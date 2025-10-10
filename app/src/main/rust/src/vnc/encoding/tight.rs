//! VNC Tight encoding implementation.
//!
//! Tight encoding with JPEG, palette, and zlib support.
//! Highly efficient for various types of screen content.

use bytes::{BufMut, BytesMut};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;
use std::collections::HashMap;
use super::Encoding;
use super::common::{rgba_to_rgb24_pixels, check_solid_color, build_palette, put_pixel32};

/// Implements the VNC "Tight" encoding with JPEG, palette, and zlib support.
pub struct TightEncoding;

impl Encoding for TightEncoding {
    fn encode(&self, data: &[u8], width: u16, height: u16, quality: u8, compression: u8) -> BytesMut {
        // Intelligently choose the best encoding method based on image content

        // Method 1: Check if it's a solid color
        let pixels = rgba_to_rgb24_pixels(data);
        if let Some(solid_color) = check_solid_color(&pixels) {
            return encode_tight_solid(solid_color);
        }

        // Method 2: Check if palette encoding would be good
        // Tight indexed color only supports 2-16 colors (RFC 6143 Section 7.7.5)
        let palette = build_palette(&pixels);
        if palette.len() >= 2 && palette.len() <= 16 && palette.len() < pixels.len() / 4 {
            return encode_tight_palette(&pixels, width, height, &palette, compression);
        }

        // Method 3: Use JPEG for photographic content (powered by libjpeg-turbo)
        encode_tight_jpeg(data, width, height, quality)
    }
}

/// Encode as Tight solid fill.
fn encode_tight_solid(color: u32) -> BytesMut {
    let mut buf = BytesMut::with_capacity(5);
    buf.put_u8(0x80); // Fill compression (solid color)
    put_pixel32(&mut buf, color); // 4 bytes for 32bpp
    buf
}

/// Encode as Tight palette.
fn encode_tight_palette(pixels: &[u32], _width: u16, _height: u16, palette: &[u32], compression: u8) -> BytesMut {
    let palette_size = palette.len();

    // Build color-to-index map
    let mut color_map: HashMap<u32, u8> = HashMap::new();
    for (idx, &color) in palette.iter().enumerate() {
        color_map.insert(color, idx as u8);
    }

    // Encode pixels as palette indices
    let mut indices = Vec::with_capacity(pixels.len());
    for &pixel in pixels {
        indices.push(*color_map.get(&pixel).unwrap_or(&0));
    }

    // Compress indices
    let compression_level = match compression {
        0 => Compression::fast(),
        1..=3 => Compression::new(compression as u32),
        4..=6 => Compression::default(),
        _ => Compression::best(),
    };

    let mut encoder = ZlibEncoder::new(Vec::new(), compression_level);
    if encoder.write_all(&indices).is_err() {
        // Compression failed, fall back to JPEG encoding
        // Convert u32 pixels back to RGBA for JPEG encoding
        return encode_tight_jpeg(
            &pixels.iter().flat_map(|&p| {
                vec![(p & 0xFF) as u8, ((p >> 8) & 0xFF) as u8, ((p >> 16) & 0xFF) as u8, 0xFF]
            }).collect::<Vec<u8>>(),
            _width, _height, 75
        );
    }
    let compressed = match encoder.finish() {
        Ok(data) => data,
        Err(_) => {
            // Compression failed, fall back to JPEG encoding
            // Convert u32 pixels back to RGBA for JPEG encoding
            return encode_tight_jpeg(
                &pixels.iter().flat_map(|&p| {
                    vec![(p & 0xFF) as u8, ((p >> 8) & 0xFF) as u8, ((p >> 16) & 0xFF) as u8, 0xFF]
                }).collect::<Vec<u8>>(),
                _width, _height, 75
            );
        }
    };

    let mut buf = BytesMut::new();

    // Compression control byte: palette compression
    buf.put_u8(0x80 | ((palette_size - 1) as u8));

    // Palette (each color is 4 bytes for 32bpp)
    for &color in palette {
        put_pixel32(&mut buf, color);
    }

    // Compact length
    let len = compressed.len();
    if len < 128 {
        buf.put_u8(len as u8);
    } else if len < 16384 {
        buf.put_u8(((len & 0x7F) | 0x80) as u8);
        buf.put_u8((len >> 7) as u8);
    } else {
        buf.put_u8(((len & 0x7F) | 0x80) as u8);
        buf.put_u8((((len >> 7) & 0x7F) | 0x80) as u8);
        buf.put_u8((len >> 14) as u8);
    }

    buf.put_slice(&compressed);
    buf
}

/// Encode as Tight JPEG using libjpeg-turbo.
fn encode_tight_jpeg(data: &[u8], width: u16, height: u16, quality: u8) -> BytesMut {
    use crate::turbojpeg::TurboJpegEncoder;

    // Convert RGBA to RGB
    let mut rgb_data = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for chunk in data.chunks_exact(4) {
        rgb_data.push(chunk[0]);
        rgb_data.push(chunk[1]);
        rgb_data.push(chunk[2]);
    }

    // Compress with TurboJPEG (libjpeg-turbo)
    let jpeg_data = match TurboJpegEncoder::new() {
        Ok(mut encoder) => {
            match encoder.compress_rgb(&rgb_data, width, height, quality) {
                Ok(data) => data,
                Err(e) => {
                    log::error!("TurboJPEG encoding failed: {}, falling back to basic tight encoding", e);
                    // Basic tight encoding requires client pixel format (4 bytes per pixel for 32bpp)
                    let mut buf = BytesMut::with_capacity(1 + data.len());
                    buf.put_u8(0x00); // Basic tight encoding, no compression
                    // Convert RGBA to client pixel format (RGBX)
                    for chunk in data.chunks_exact(4) {
                        buf.put_u8(chunk[0]); // R
                        buf.put_u8(chunk[1]); // G
                        buf.put_u8(chunk[2]); // B
                        buf.put_u8(0);        // Padding
                    }
                    return buf;
                }
            }
        }
        Err(e) => {
            log::error!("Failed to create TurboJPEG encoder: {}, falling back to basic tight encoding", e);
            // Basic tight encoding requires client pixel format (4 bytes per pixel for 32bpp)
            let mut buf = BytesMut::with_capacity(1 + data.len());
            buf.put_u8(0x00); // Basic tight encoding, no compression
            // Convert RGBA to client pixel format (RGBX)
            for chunk in data.chunks_exact(4) {
                buf.put_u8(chunk[0]); // R
                buf.put_u8(chunk[1]); // G
                buf.put_u8(chunk[2]); // B
                buf.put_u8(0);        // Padding
            }
            return buf;
        }
    };

    let mut buf = BytesMut::new();
    buf.put_u8(0x90); // JPEG subencoding

    // Compact length
    let len = jpeg_data.len();
    if len < 128 {
        buf.put_u8(len as u8);
    } else if len < 16384 {
        buf.put_u8(((len & 0x7F) | 0x80) as u8);
        buf.put_u8((len >> 7) as u8);
    } else {
        buf.put_u8(((len & 0x7F) | 0x80) as u8);
        buf.put_u8((((len >> 7) & 0x7F) | 0x80) as u8);
        buf.put_u8((len >> 14) as u8);
    }

    buf.put_slice(&jpeg_data);
    buf
}

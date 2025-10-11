# VNC Encoding Comparison: Rust vs libvncserver

## libvncserver Encodings (from rfbproto.h):
- Raw (0)
- CopyRect (1) 
- RRE (2)
- CoRRE (4)
- Hextile (5)
- Zlib (6)
- Tight (7)
- TightPng (-260)
- ZlibHex (8)
- Ultra (9)
- TRLE (15)
- ZRLE (16)
- ZYWRLE (17)
- H264 (0x48323634)
- Various cache/XOR encodings
- Cursor pseudo-encoding (-239)
- Desktop Size pseudo-encoding (-223)

## Rust VNC Implemented Encodings:
- Raw (0) ✅
- RRE (2) ✅
- CoRRE (4) ✅
- Hextile (5) ✅
- Zlib (6) ✅
- Tight (7) ✅
- ZRLE (16) ✅ (implemented but unused)
- Cursor pseudo-encoding (-239) ✅ (defined)
- Desktop Size pseudo-encoding (-223) ✅ (defined)

## Missing from Rust Implementation:
1. CopyRect (1) - Copy rectangle from another part of framebuffer
2. TightPng (-260) - Tight encoding with PNG compression
3. ZlibHex (8) - Zlib compressed Hextile
4. Ultra (9) - UltraVNC's proprietary encoding
5. TRLE (15) - Tiled Run-Length Encoding
6. ZYWRLE (17) - Wavelet-based lossy compression
7. H264 (0x48323634) - H.264 video encoding
8. Cache/XOR encodings - Various TurboVNC optimizations

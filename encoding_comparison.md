# VNC Encoding Comparison: RustVNC vs libvncserver

This document provides a detailed comparison of VNC encodings between the Rust implementation (RustVNC) and the C-based libvncserver implementation used in droidVNC-NG v2.15.0.

## Encoding Priority Order

### RustVNC Encoding Selection Priority
When a client supports multiple encodings, RustVNC selects them in this order:
1. **Zlib** (6) - Highest priority for good compression with moderate CPU usage
2. **ZlibHex** (8) - Zlib-compressed Hextile for efficient tile-based compression
3. **ZRLE** (16) - Zlib Run-Length Encoding with 64x64 tiles and palette compression
4. **Tight** (7) - JPEG-based compression via libjpeg-turbo for photo-realistic content
5. **Hextile** (5) - Tile-based encoding with minimal CPU overhead
6. **Raw** (0) - Fallback uncompressed encoding

### CopyRect Handling
- **CopyRect** (1) is handled separately with **highest priority**
- Sent BEFORE any other encoding type when explicitly scheduled
- Used for scrolling, window dragging, and screen region movement

---

## Fully Implemented and Actively Used Encodings

### 1. Raw (0) ✅
**Status:** Fully implemented, used as fallback
- Uncompressed pixel data transmission
- Highest bandwidth but universally supported
- Used when no better encoding is available or encoding fails

**Implementation:**
- File: `src/vnc/encoding/raw.rs`
- Simple direct pixel copy, no compression

---

### 2. CopyRect (1) ✅
**Status:** Fully implemented with libvncserver parity
- Instructs client to copy rectangle from one screen location to another
- Extremely efficient for scrolling and window dragging operations
- **Only 8 bytes overhead** per rectangle (src_x, src_y coordinates)

**Implementation:**
- Per-client tracking of copy regions and offset (dx, dy)
- Explicit scheduling via `vncScheduleCopyRect()` and `vncDoCopyRect()` JNI methods
- Matches libvncserver's `rfbScheduleCopyRect` and `rfbDoCopyRect` behavior
- Copy regions sent FIRST, before modified regions
- Handles offset conflicts (different offsets convert old copies to modifications)

**Files:**
- `src/vnc/client.rs` - Client-level copy region tracking and sending
- `src/vnc/server.rs` - Server-level scheduling methods
- `src/vnc/framebuffer.rs` - Actual framebuffer memory copy with overlap handling
- `src/vnc_jni.rs` - JNI bindings for Java layer

**Algorithm:**
```
For each framebuffer update:
1. Send all CopyRect regions with stored (dx, dy) offset
2. Then send modified regions using other encodings
```

---

### 3. RRE - Rise-and-Run-length Encoding (2) ✅
**Status:** Fully implemented
- Simple compression using background color + rectangular subrects
- Good for solid color blocks and simple graphics

**Implementation:**
- File: `src/vnc/encoding/rre.rs`
- Registered in encoding registry

---

### 4. CoRRE - Compact RRE (4) ✅
**Status:** Fully implemented
- More compact version of RRE using 8-bit coordinates
- Better for small rectangles

**Implementation:**
- File: `src/vnc/encoding/corre.rs`
- Similar to RRE but with smaller coordinate representation

---

### 5. Hextile (5) ✅
**Status:** Fully implemented, actively used (3rd priority)
- Divides rectangles into 16x16 tiles
- Each tile can use raw, solid color, or subrectangle encoding
- Good balance of compression and CPU usage

**Implementation:**
- File: `src/vnc/encoding/hextile.rs`
- Supports all Hextile sub-encoding types:
  - Raw tiles
  - Background color specified
  - Foreground color specified
  - Subrectangles (colored and monochrome)

---

### 6. Zlib (6) ✅
**Status:** Fully implemented, actively used (HIGHEST PRIORITY)
- Zlib compression on raw pixel data
- **Persistent compression stream** per client connection (RFC 6143 compliant)
- Adjustable compression level (0-9) via pseudo-encoding

**Implementation:**
- File: `src/vnc/encoding/zlib.rs`
- Uses `flate2` crate for zlib compression
- Maintains per-client `Compress` instance for streaming compression
- Compression level controlled by client's `ENCODING_COMPRESS_LEVEL_*` pseudo-encodings

**Features:**
- Persistent compression dictionary across updates
- Better compression ratios than one-shot compression
- Matches libvncserver's persistent stream behavior

---

### 7. Tight (7) ✅
**Status:** Fully implemented with JPEG support (2nd priority)
- Highly efficient encoding using multiple compression methods
- **JPEG compression via libjpeg-turbo** for gradient/photo content
- Adjustable JPEG quality (1-100) via pseudo-encoding
- Uses 4:2:2 chroma subsampling for good quality/size balance

**Implementation:**
- File: `src/vnc/encoding/tight.rs`
- File: `src/turbojpeg.rs` - FFI bindings to libjpeg-turbo
- Quality level controlled by client's `ENCODING_QUALITY_LEVEL_*` pseudo-encodings
- Maps TightVNC quality levels (0-9) to libjpeg-turbo quality (15-100)

**Quality Mapping (libvncserver compatible):**
```
Level 0: 15%  | Level 5: 77%
Level 1: 29%  | Level 6: 79%
Level 2: 41%  | Level 7: 86%
Level 3: 42%  | Level 8: 92%
Level 4: 62%  | Level 9: 100%
```

**JPEG Compilation:**
- libjpeg-turbo compiled from source during build
- Optimized assembly implementations (NEON on ARM, SSE2 on x86)
- Statically linked into final binary

---

### 8. ZRLE - Zlib Run-Length Encoding (16) ✅
**Status:** Fully implemented, actively used (2nd priority)
- Combines 64x64 tile-based encoding with palette compression and run-length encoding
- **Persistent compression stream** per client connection (RFC 6143 compliant)
- Multiple sub-encodings: solid, raw, packed palette, palette RLE, plain RLE
- CPIXEL (3-byte RGB) format for efficient transmission
- Adjustable compression level (0-9) via pseudo-encoding

**Implementation:**
- File: `src/vnc/encoding/zrle.rs`
- Uses `flate2` crate for zlib compression
- Maintains per-client `Compress` instance for streaming compression
- Compression level controlled by client's `ENCODING_COMPRESS_LEVEL_*` pseudo-encodings

**Features:**
- Persistent compression dictionary across updates
- Intelligent palette detection and RLE for uniform regions
- Better compression than Zlib for certain content types (text, UI)
- Matches libvncserver's persistent stream behavior

---

### 9. ZlibHex - Zlib-compressed Hextile (8) ✅
**Status:** Fully implemented, actively used (2nd priority)
- Combines Hextile's 16x16 tile-based encoding with zlib compression
- **Persistent compression stream** per client connection (RFC 6143 compliant)
- Efficient for UI content with repeated patterns
- Adjustable compression level (0-9) via pseudo-encoding

**Implementation:**
- File: `src/vnc/encoding/zlibhex.rs`
- Uses `flate2` crate for zlib compression
- First applies Hextile encoding, then compresses the result
- Maintains per-client `Compress` instance for streaming compression
- Compression level controlled by client's `ENCODING_COMPRESS_LEVEL_*` pseudo-encodings

**Features:**
- Persistent compression dictionary across updates
- Better compression than plain Hextile
- Lower CPU overhead than ZRLE for simple UI content
- Matches libvncserver's persistent stream behavior

---

## Pseudo-Encodings (Fully Supported)

### Quality Level Pseudo-Encodings (-32 to -23) ✅
**Status:** Fully implemented and respected
- Client can request specific JPEG quality levels
- Affects Tight encoding JPEG quality
- Values: `ENCODING_QUALITY_LEVEL_0` (-32) to `ENCODING_QUALITY_LEVEL_9` (-23)

**Usage:** When client includes these in encoding list, server adjusts JPEG quality accordingly.

---

### Compression Level Pseudo-Encodings (-256 to -247) ✅
**Status:** Fully implemented and respected
- Client can request specific zlib compression levels
- Affects Zlib and Tight encodings
- Values: `ENCODING_COMPRESS_LEVEL_0` (-256) to `ENCODING_COMPRESS_LEVEL_9` (-247)

**Usage:** When client includes these in encoding list, server adjusts zlib compression level accordingly.

---

## Defined but NOT Implemented

### Cursor Pseudo-Encoding (-239) ⚠️
**Status:** Constant defined only
- `ENCODING_CURSOR` constant exists in protocol.rs
- NO implementation code
- Cursor shapes are not transmitted to clients

---

### Desktop Size Pseudo-Encoding (-223) ⚠️
**Status:** Constant defined only
- `ENCODING_DESKTOP_SIZE` constant exists in protocol.rs
- NO implementation code
- Clients are not notified of framebuffer size changes
- Server uses `rfbNewFramebuffer` equivalent but doesn't send Desktop Size message

---

### TRLE - Tile Run-Length Encoding (15) ⚠️
**Status:** Constant defined only
- `ENCODING_TRLE` constant exists in protocol.rs
- NO implementation code

---

### H.264 - H.264 Video Encoding (0x48323634) ⚠️
**Status:** Constant defined only (matching libvncserver)
- `ENCODING_H264` constant exists in protocol.rs
- NO implementation code
- **Note:** libvncserver removed H.264 implementation in v0.9.11 (2016-12-30) because it was "broken and unmaintained"
- Constant exists for RFB protocol compatibility only

---

## Not Implemented (libvncserver has these)

The following encodings are available in libvncserver but completely absent from RustVNC:

1. **TightPng (-260)** - Tight encoding with PNG compression instead of JPEG
2. **Ultra (9)** - UltraVNC's proprietary encoding
3. **ZYWRLE (17)** - Wavelet-based lossy compression for low-bandwidth
4. **Cache encodings** - Various TurboVNC cache-based optimizations
5. **XOR encodings** - TurboVNC XOR-based optimizations

**Note:** H.264 (0x48323634) is NOT in this list because libvncserver also does not implement it (removed in 2016). RustVNC matches libvncserver by having the constant defined but not implemented.

---

## Summary Statistics

### By Implementation Status

| Status | Count | Encodings |
|--------|-------|-----------|
| **Fully Implemented & Active** | 9 | Raw, CopyRect, RRE, CoRRE, Hextile, Zlib, ZlibHex, Tight, ZRLE |
| **Defined but Not Implemented** | 4 | Cursor, Desktop Size, TRLE, H.264 |
| **Not Implemented** | 6+ | TightPng, Ultra, ZYWRLE, Cache, XOR, etc. |

### Key Differences from libvncserver

**✅ Advantages:**
- CopyRect fully implemented with libvncserver parity
- ZlibHex encoding now actively used (2nd priority, excellent for UI content)
- ZRLE encoding actively used (3rd priority, good for text/UI)
- JPEG compression via optimized libjpeg-turbo (same as libvncserver)
- Persistent Zlib, ZlibHex, and ZRLE compression streams (RFC 6143 compliant)
- Quality and compression level pseudo-encodings fully supported
- Simpler, more maintainable codebase

**❌ Missing Features:**
- No cursor shape updates
- No desktop size change notifications
- No advanced encodings (ZYWRLE, TightPng)
- No cache or XOR optimizations
- H.264 constant defined but not implemented (matches libvncserver - removed in 2016)

---

## Performance Characteristics

### Typical Encoding Usage by Scenario

**Text editing / Terminal:**
- Primary: **ZlibHex** (excellent for text/UI with tile compression)
- Secondary: **ZRLE** (excellent for text/UI with palette compression)
- Alternative: **Hextile** (good for text)
- Fallback: **Raw**

**Web browsing / Photos:**
- Primary: **Tight with JPEG** (excellent compression for photos)
- Secondary: **Zlib** (if no gradients)
- Alternative: **ZlibHex** or **ZRLE** (good for UI elements)
- CopyRect: **Scrolling operations**

**Video playback / Gaming:**
- Primary: **Zlib** (fast compression)
- Secondary: **Raw** (lowest latency)
- **Note:** H264 encoding would be better but not implemented

**Window dragging / Scrolling:**
- **CopyRect** - Ultra-efficient, only 8 bytes per rectangle

### Bandwidth Comparison (Typical)

For a 1920x1080 RGBA32 framebuffer full update:
- **Raw:** ~8.3 MB
- **Zlib (level 6):** ~500 KB - 2 MB (depends on content)
- **ZlibHex (level 6):** ~400 KB - 1.8 MB (better for UI, compressed Hextile)
- **ZRLE (level 6):** ~300 KB - 1.5 MB (better for text/UI, palette compression)
- **Tight (quality 90):** ~100 KB - 500 KB (photo content)
- **Hextile:** ~1-3 MB (text/UI content)
- **CopyRect:** **8 bytes** (just coordinates!)

---

## Future Improvements

### High Priority
1. **Implement Desktop Size pseudo-encoding** - Notify clients of resize
2. **Implement Cursor pseudo-encoding** - Reduce cursor rendering overhead

### Medium Priority
1. **TightPng encoding** - PNG compression for lossless quality
2. **ZYWRLE encoding** - For very low bandwidth scenarios

**Note:** H.264 encoding is intentionally NOT on this list. Both RustVNC and libvncserver define the constant but don't implement H.264 (libvncserver removed it in 2016 as "broken and unmaintained").

### Low Priority
1. Cache-based optimizations
2. XOR-based optimizations

---

## Conclusion

RustVNC now implements **9 actively used encodings** that cover the vast majority of real-world VNC usage scenarios. The implementation includes:
- **CopyRect** with full libvncserver parity for ultra-efficient scrolling/window dragging
- **ZlibHex** for excellent UI compression combining Hextile tiles with zlib compression
- **ZRLE** for excellent text/UI compression with palette detection and run-length encoding
- **Tight with JPEG** for photo-realistic content via libjpeg-turbo
- **Zlib** for fast general-purpose compression

The implementation prioritizes:
- **Compatibility** with standard VNC clients
- **Performance** via libjpeg-turbo and persistent compression streams
- **Efficiency** through intelligent encoding selection (ZLIB > ZLIBHEX > ZRLE > TIGHT > HEXTILE)
- **Simplicity** by focusing on proven, widely-supported encodings
- **Correctness** by matching libvncserver's behavior exactly (RFC 6143 compliant)

For most use cases, RustVNC's encoding support is **sufficient and well-optimized**. Advanced encodings like ZYWRLE and TightPng would benefit specialized scenarios but are not critical for general desktop sharing.

**H.264 Status:** Both RustVNC and libvncserver define the H.264 encoding constant (0x48323634) for RFB protocol compatibility, but neither implements the actual encoding. libvncserver removed H.264 in version 0.9.11 (2016) because the implementation was "broken and unmaintained". RustVNC maintains protocol parity by defining the constant without implementation.

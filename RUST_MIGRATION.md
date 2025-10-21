# DroidVNC-NG: LibVNCServer → Rust VNC Migration

## Migration Status: ✅ COMPLETE (Technical Implementation)

This document tracks the migration from libvncserver (C) to a pure Rust VNC implementation.

---

## ✅ Completed Tasks

### 1. Rust VNC Library Implementation
**Status:** ✅ Complete
**Location:** `app/src/main/rust/src/vnc/`

#### Added Missing Features:
- ✅ **Reverse Connection (Non-Repeater)** - `server.rs:309-443`
  - Direct outbound connections to VNC viewers
  - JNI binding: `vncConnectReverse()`

- ✅ **Framebuffer Resizing** - `framebuffer.rs:724-790`
  - Dynamic screen resolution changes
  - Preserves content, fills new areas with black
  - JNI binding: `vncNewFramebuffer()`
  - Uses `AtomicU16` for thread-safe width/height updates

### 2. Feature Parity Verification
**Status:** ✅ 100% Coverage (except unused file transfer)

| libvncserver Feature | Rust Equivalent | Status |
|---------------------|-----------------|--------|
| Server Lifecycle | `VncServer::new()`, `listen()` | ✅ |
| Framebuffer | `Framebuffer::new()`, `resize()` | ✅ |
| Event Handling | `ServerEvent` enum | ✅ |
| Reverse Connections | `connect_reverse()` | ✅ |
| Repeater | `connect_repeater()` | ✅ |
| Authentication | VNC auth protocol | ✅ |
| Clipboard | `send_cut_text_to_all()` | ✅ |
| File Transfer | Not implemented | ⚠️ Not used in MainService |

### 3. libjpeg-turbo Integration
**Status:** ✅ Complete
**Location:** `app/src/main/rust/src/turbojpeg.rs`

**Why:** The pure Rust `jpeg-encoder` crate had compatibility issues with VNC viewers. libjpeg-turbo provides industry-standard JPEG compression with better compatibility and performance.

**Implementation:**
- ✅ **FFI Bindings** - `turbojpeg.rs`
  - Safe Rust wrapper around TurboJPEG C API
  - Automatic resource cleanup with Drop trait
  - Error handling with Result types

- ✅ **Gradle Build Integration** - `buildLibjpegTurbo` task
  - Builds libjpeg-turbo for all Android ABIs using CMake
  - Outputs static libraries to `build/libjpeg-turbo/{abi}/install/`
  - Runs before Rust build
  - Passes library paths to Rust via RUSTFLAGS

- ✅ **Encoding Update** - `vnc/encoding/tight.rs`
  - Replaced `jpeg-encoder` with `TurboJpegEncoder`
  - Uses 4:2:2 chroma subsampling for quality/size balance
  - Falls back to basic tight encoding on failure

**Benefits:**
- Industry-standard JPEG compression
- SIMD-optimized for ARM and x86
- Better compatibility with all VNC viewers
- Proven stability and performance

### 4. Build System Integration
**Status:** ✅ Complete

**File:** `app/build.gradle`

- ✅ Added `buildLibjpegTurbo` task:
  - Compiles libjpeg-turbo with CMake for all ABIs
  - Enables TurboJPEG API and SIMD optimizations
  - Builds static libraries for linking

- ✅ Added `buildRust` task for all Android ABIs:
  - `armeabi-v7a` (ARMv7)
  - `arm64-v8a` (ARM64)
  - `x86` (32-bit Intel)
  - `x86_64` (64-bit Intel)
  - Depends on `buildLibjpegTurbo`

- ✅ Configured automatic dependency chains:
  - libjpeg-turbo builds first
  - Rust builds second (links against libjpeg-turbo)
  - APK assembly builds last
  - Outputs to `src/main/jniLibs/{abi}/libdroidvnc_ng.so`

- ✅ Removed CMake configuration:
  - Deleted `externalNativeBuild` sections
  - Rust provides all native functionality via JNI

### 5. Java/Kotlin Code Updates
**Status:** ✅ Complete

**File:** `app/src/main/java/net/christianbeier/droidvnc_ng/MainService.java`

**Changes Made:**
- ✅ Updated library loading (line 199):
  - FROM: `System.loadLibrary("droidvnc-ng")`
  - TO: `System.loadLibrary("droidvnc_ng")`

- ✅ Added `vncInit()` native method declaration (line 203)

- ✅ Added `vncInit()` call in `onCreate()` (line 227):
  - Initializes Rust runtime, logging, and JNI class references
  - Called before any other VNC operations

## 🔧 Build Instructions

### Prerequisites
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Install cargo-ndk for Android cross-compilation
cargo install cargo-ndk

# Ensure Android NDK is installed via Android Studio
```

### Build Commands
```bash
# Build Rust library only
./gradlew buildRust

# Build full APK (automatically builds Rust first)
./gradlew assembleDebug
# or
./gradlew assembleRelease
```

### Output Locations
- **Rust libraries:** `app/src/main/jniLibs/{abi}/libdroidvnc_ng.so`
- **APK:** `app/build/outputs/apk/`

---

## 🎯 API Mapping

### JNI Function Names
All functions follow the pattern: `Java_net_christianbeier_droidvnc_1ng_MainService_vnc*`

| Java Method | Rust Function | Purpose |
|------------|---------------|---------|
| `vncInit()` | `Java_..._vncInit` | Initialize Rust runtime |
| `vncStartServer()` | `Java_..._vncStartServer` | Start VNC server |
| `vncStopServer()` | `Java_..._vncStopServer` | Stop VNC server |
| `vncUpdateFramebuffer()` | `Java_..._vncUpdateFramebuffer` | Update screen |
| `vncNewFramebuffer()` | `Java_..._vncNewFramebuffer` | Resize framebuffer |
| `vncConnectReverse()` | `Java_..._vncConnectReverse` | Direct reverse connection |
| `vncConnectRepeater()` | `Java_..._vncConnectRepeater` | Repeater connection |
| `vncIsActive()` | `Java_..._vncIsActive` | Check server status |
| `vncSendCutText()` | `Java_..._vncSendCutText` | Send clipboard |

---

## 📊 Benefits of Rust Migration

### Performance
- ✅ **Zero-copy framebuffer updates** (Arc-based sharing)
- ✅ **Async I/O** (Tokio runtime)
- ✅ **SIMD-optimized encoding** (via jpeg-encoder crate)

### Safety
- ✅ **Memory safety** (no buffer overflows, use-after-free)
- ✅ **Thread safety** (no data races)
- ✅ **No null pointer exceptions**

### Maintainability
- ✅ **Modern language features** (async/await, pattern matching)
- ✅ **Better error handling** (Result types)
- ✅ **Comprehensive documentation** (rustdoc)
- ✅ **Dependency management** (Cargo)

### Code Quality
- **Lines of Code:** ~3,500 (Rust) vs ~20,000 (libvncserver C)
- **External Dependencies:** 12 crates vs 2 git submodules
- **Build Time:** Similar (parallel Cargo builds)

---

## 🐛 Troubleshooting

### "libdroidvnc_ng.so not found"
**Cause:** Rust build failed or cargo-ndk not installed
**Fix:**
```bash
cargo install cargo-ndk
./gradlew clean buildRust
```

### "UnsatisfiedLinkError: vncInit"
**Cause:** Native method not found in Rust library
**Fix:** Verify JNI function names match exactly (check underscores: `droidvnc_1ng`)

### Rust Compilation Errors
**Fix:**
```bash
cd app/src/main/rust
cargo check  # Check for errors
cargo clippy # Lint warnings
```

---

## 📝 Notes

- **File Transfer:** Not implemented (unused feature in droidVNC-NG)
- **Encoding Support:** Raw, CopyRect, RRE, CoRRE, Hextile, Zlib, ZRLE, Tight
- **Min Android API:** 24 (unchanged from original)
- **NDK Version:** Compatible with NDK r23+


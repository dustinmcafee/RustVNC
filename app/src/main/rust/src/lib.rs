//! DroidVNC-NG Rust VNC Server Library
//!
//! This crate provides a pure Rust VNC server implementation for Android,
//! using the standalone rustvncserver library.
//!
//! # Modules
//!
//! - `vnc_jni`: Provides the JNI bindings for the VNC server to interface with Java code.
//! - `turbojpeg`: FFI bindings to libjpeg-turbo for JPEG compression.
//!
//! # Architecture
//!
//! The VNC server functionality is provided by the `rustvncserver` library,
//! which is a standalone, platform-agnostic VNC server implementation.
//! This crate provides the Android-specific JNI bindings to expose the
//! server to Java code.

mod vnc_jni;
mod turbojpeg;

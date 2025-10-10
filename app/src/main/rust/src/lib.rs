//! DroidVNC-NG Rust VNC Server Library
//!
//! This crate provides a pure Rust VNC server implementation for Android,
//! designed to replace libvncserver with better performance, safety, and maintainability.
//!
//! # Modules
//!
//! - `vnc`: Contains the core VNC server implementation.
//! - `vnc_jni`: Provides the JNI bindings for the VNC server to interface with Java code.
//! - `turbojpeg`: FFI bindings to libjpeg-turbo for JPEG compression.

mod vnc;
mod vnc_jni;
mod turbojpeg;

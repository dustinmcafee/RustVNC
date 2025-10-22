// Copyright 2025 Dustin McAfee
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.


//! DroidVNC-NG Rust VNC Server Library
//!
//! This crate provides a pure Rust VNC server implementation for Android,
//! using the standalone rustvncserver library.
//!
//! # Modules
//!
//! - `vnc_jni`: Provides the JNI bindings for the VNC server to interface with Java code.
//!
//! # Architecture
//!
//! The VNC server functionality is provided by the `rustvncserver` library,
//! which is a standalone, platform-agnostic VNC server implementation.
//! This crate provides the Android-specific JNI bindings to expose the
//! server to Java code.

mod vnc_jni;

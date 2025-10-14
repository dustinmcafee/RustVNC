//! Core VNC (Virtual Network Computing) server implementation.
//!
//! This module provides a complete Remote Framebuffer (RFB) protocol implementation
//! for serving desktop screens over the network. It is specifically optimized for
//! Android ARM64 platforms but can be used in other contexts as well.
//!
//! # Architecture
//!
//! The VNC implementation is organized into several key components:
//!
//! - **`protocol`**: RFB protocol constants, message types, and data structures
//! - **`server`**: Main server logic for accepting connections and managing clients
//! - **`client`**: Per-client session handling and message processing
//! - **`framebuffer`**: Screen buffer management and dirty region tracking
//! - **`encoding`**: Pixel data encoding strategies (Raw, Tight, etc.)
//! - **`auth`**: VNC authentication implementation
//! - **`repeater`**: Support for VNC repeater/reverse connections
//!
//! # Features
//!
//! - **Multiple Encodings**: Supports Raw and Tight (JPEG) encodings
//! - **Copy Rectangle**: Automatic detection of scrolling for bandwidth optimization
//! - **Rate Limiting**: Prevents overwhelming clients with excessive updates
//! - **Batched Updates**: Combines multiple dirty regions for efficient transmission
//! - **Repeater Support**: Enables connections through NAT/firewalls
//! - **Push-based Updates**: Framebuffer automatically notifies clients of changes
//!
//! # Example Flow
//!
//! ```ignore
//! // Create server
//! let (server, event_rx) = VncServer::new(1920, 1080, "My Desktop".to_string(), None);
//!
//! // Start listening
//! tokio::spawn(async move {
//!     server.listen(5900).await.unwrap();
//! });
//!
//! // Update framebuffer
//! server.framebuffer().update_from_slice(&pixel_data).await.unwrap();
//! ```

pub mod protocol;
pub mod server;
pub mod framebuffer;
pub mod encoding;
pub mod auth;
pub mod client;
pub mod repeater;
pub mod translate;


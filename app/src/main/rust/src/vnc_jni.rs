//! JNI bindings for the Rust VNC Server, allowing it to be controlled from a Java/Android application.
//!
//! This module exposes functions to initialize, start, stop, and interact with the VNC server.
//! It manages a global Tokio runtime, a VNC server instance, and handles the forwarding of
//! VNC events (like client connections, disconnections, and input events) to the Java layer.

use jni::JNIEnv;
use jni::objects::{JClass, JString, JObject, JValue};
use jni::sys::{jint, jboolean, jlong, JNI_TRUE, JNI_FALSE};
use log::{info, error, warn};
use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, broadcast};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::vnc::server::VncServer;
use crate::vnc::server::ServerEvent;

/// Global Tokio runtime for the VNC server.
static VNC_RUNTIME: OnceCell<Runtime> = OnceCell::new();
/// Global container for the VNC server instance.
static VNC_SERVER: OnceCell<Arc<Mutex<Option<Arc<VncServer>>>>> = OnceCell::new();
/// Global broadcast sender for shutdown signals.
static SHUTDOWN_SIGNAL: OnceCell<broadcast::Sender<()>> = OnceCell::new();
/// Atomic flag to track if the event handler is running.
static EVENT_HANDLER_RUNNING: AtomicBool = AtomicBool::new(false);

// Store Java VM and class references
/// Global reference to the Java VM.
static JAVA_VM: OnceCell<jni::JavaVM> = OnceCell::new();
/// Global reference to the `InputService` Java class.
static INPUT_SERVICE_CLASS: OnceCell<jni::objects::GlobalRef> = OnceCell::new();
/// Global reference to the `MainService` Java class.
static MAIN_SERVICE_CLASS: OnceCell<jni::objects::GlobalRef> = OnceCell::new();

// Unique client ID counter
#[allow(dead_code)]
static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);

/// Initializes or retrieves the global Tokio runtime for the VNC server.
///
/// This function ensures that a single instance of the Tokio multi-threaded runtime
/// is created and shared across the VNC server module.
///
/// # Returns
///
/// A static reference to the initialized `tokio::runtime::Runtime`.
fn get_or_init_vnc_runtime() -> &'static Runtime {
    VNC_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build VNC Tokio runtime")
    })
}

/// Initializes or retrieves the global broadcast sender for shutdown signals.
///
/// This function creates a broadcast channel for sending shutdown notifications to all
/// active asynchronous tasks within the VNC server.
///
/// # Returns
///
/// A static reference to the `broadcast::Sender<()>`.
fn get_or_init_shutdown_signal() -> &'static broadcast::Sender<()> {
    SHUTDOWN_SIGNAL.get_or_init(|| {
        let (tx, _) = broadcast::channel(16);
        tx
    })
}

/// JNI entry point to initialize the VNC server's Rust components.
///
/// This function should be called once when the Android application starts. It initializes
/// the Tokio runtime, the shutdown signal, the server container, and caches references to
/// the Java VM and required Java classes for later use.
///
/// # Arguments
///
/// * `env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncInit(
    mut env: JNIEnv,
    _class: JClass,
) {
    // Initialize Android logger first so we can see logs
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("DroidVNC-Rust")
    );

    info!("Initializing Rust VNC Server");

    // Initialize the runtime
    get_or_init_vnc_runtime();
    get_or_init_shutdown_signal();

    // Store Java VM reference
    if let Ok(vm) = env.get_java_vm() {
        let _ = JAVA_VM.set(vm);
    }

    // Get and store class references
    if let Ok(input_class) = env.find_class("net/christianbeier/droidvnc_ng/InputService") {
        if let Ok(global_ref) = env.new_global_ref(input_class) {
            let _ = INPUT_SERVICE_CLASS.set(global_ref);
        }
    }

    if let Ok(main_class) = env.find_class("net/christianbeier/droidvnc_ng/MainService") {
        if let Ok(global_ref) = env.new_global_ref(main_class) {
            let _ = MAIN_SERVICE_CLASS.set(global_ref);
        }
    }

    // Initialize server container
    VNC_SERVER.get_or_init(|| Arc::new(Mutex::new(None)));

    info!("Rust VNC Server initialized");
}

/// JNI entry point to start the VNC server.
///
/// This function creates a new `VncServer` instance with the specified parameters,
/// stores it in a global container, and starts the server's listener and event handler tasks.
///
/// # Arguments
///
/// * `env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
/// * `width` - The width of the framebuffer.
/// * `height` - The height of the framebuffer.
/// * `port` - The TCP port on which the server will listen.
/// * `desktop_name` - The name of the VNC desktop.
/// * `password` - The password for VNC authentication.
///
/// # Returns
///
/// `JNI_TRUE` if the server starts successfully, `JNI_FALSE` otherwise.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncStartServer(
    mut env: JNIEnv,
    _class: JClass,
    width: jint,
    height: jint,
    port: jint,
    desktop_name: JString,
    password: JString,
) -> jboolean {
    // Validate dimensions to prevent integer overflow (R3)
    const MAX_DIMENSION: i32 = 8192;
    const MIN_DIMENSION: i32 = 1;

    let width = match u16::try_from(width) {
        Ok(w) if w >= MIN_DIMENSION as u16 && w <= MAX_DIMENSION as u16 => w,
        _ => {
            error!("Invalid width: {} (must be {}-{})", width, MIN_DIMENSION, MAX_DIMENSION);
            return JNI_FALSE;
        }
    };

    let height = match u16::try_from(height) {
        Ok(h) if h >= MIN_DIMENSION as u16 && h <= MAX_DIMENSION as u16 => h,
        _ => {
            error!("Invalid height: {} (must be {}-{})", height, MIN_DIMENSION, MAX_DIMENSION);
            return JNI_FALSE;
        }
    };

    // Port -1 means "inbound connections disabled" - server will only support outbound connections
    let port_opt: Option<u16> = if port == -1 {
        None
    } else {
        match u16::try_from(port) {
            Ok(p) if p > 0 => Some(p),
            _ => {
                error!("Invalid port: {} (must be -1 for disabled or 1-65535)", port);
                return JNI_FALSE;
            }
        }
    };

    let desktop_name_str: String = match env.get_string(&desktop_name) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get desktop name: {}", e);
            return JNI_FALSE;
        }
    };

    let password_str: Option<String> = if !password.is_null() {
        match env.get_string(&password) {
            Ok(s) => {
                let pw: String = s.into();
                if pw.is_empty() {
                    None
                } else {
                    Some(pw)
                }
            }
            Err(_) => None,
        }
    } else {
        None
    };

    if let Some(p) = port_opt {
        info!("Starting Rust VNC Server: {}x{} on port {}", width, height, p);
    } else {
        info!("Starting Rust VNC Server: {}x{} (inbound connections disabled)", width, height);
    }

    // Create server and event receiver
    let (server, event_rx) = VncServer::new(width, height, desktop_name_str, password_str);
    let server: Arc<VncServer> = Arc::new(server);

    // Store the server globally
    if let Some(server_container) = VNC_SERVER.get() {
        match server_container.lock() {
            Ok(mut guard) => {
                *guard = Some(server.clone());
            }
            Err(e) => {
                error!("Failed to lock server container: {}", e);
                return JNI_FALSE;
            }
        }
    } else {
        error!("VNC server container not initialized");
        return JNI_FALSE;
    }

    // Start event handler FIRST
    spawn_event_handler(event_rx);

    // Start listener only if port is specified (not -1)
    if let Some(listen_port) = port_opt {
        let runtime = get_or_init_vnc_runtime();
        let server_clone = server.clone();
        let mut shutdown_rx = get_or_init_shutdown_signal().subscribe();

        runtime.spawn(async move {
            tokio::select! {
                result = server_clone.listen(listen_port) => {
                    if let Err(e) = result {
                        error!("VNC server listen error: {}", e);
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("VNC server received shutdown signal");
                }
            }
        });
    } else {
        info!("VNC server running in outbound-only mode (no listener)");
    }

    info!("Rust VNC Server started successfully");
    JNI_TRUE
}

/// JNI entry point to stop the VNC server.
///
/// This function sends a shutdown signal to all active server tasks and clears the global
/// server reference, effectively stopping the VNC server.
///
/// # Arguments
///
/// * `_env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
///
/// # Returns
///
/// `JNI_TRUE` to indicate that the stop command was issued.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncStopServer(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    info!("Stopping Rust VNC Server");

    // Send shutdown signal to all tasks
    if let Some(shutdown_tx) = SHUTDOWN_SIGNAL.get() {
        let _ = shutdown_tx.send(());
    }

    // Clear server reference
    if let Some(server_container) = VNC_SERVER.get() {
        if let Ok(mut guard) = server_container.lock() {
            *guard = None;
        }
    }

    // Reset event handler flag
    EVENT_HANDLER_RUNNING.store(false, Ordering::SeqCst);

    info!("Rust VNC Server stopped");
    JNI_TRUE
}

/// JNI entry point to update the entire framebuffer with new screen data.
///
/// This function receives a direct `ByteBuffer` from Java containing the new framebuffer image.
/// It copies the data into a Rust-owned buffer and then calls the VNC server's
/// `update_from_slice` method to update the framebuffer and mark the modified regions as dirty.
///
/// # Arguments
///
/// * `env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
/// * `buffer` - A Java `DirectByteBuffer` containing the new RGBA framebuffer data.
///
/// # Returns
///
/// `JNI_TRUE` if the update is successful, `JNI_FALSE` otherwise.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncUpdateFramebuffer(
    env: JNIEnv,
    _class: JClass,
    buffer: JObject,
) -> jboolean {
    let buffer_ptr = match env.get_direct_buffer_address((&buffer).into()) {
        Ok(ptr) => ptr,
        Err(e) => {
            error!("Failed to get buffer address: {}", e);
            return JNI_FALSE;
        }
    };

    let buffer_capacity = match env.get_direct_buffer_capacity((&buffer).into()) {
        Ok(cap) => cap,
        Err(e) => {
            error!("Failed to get buffer capacity: {}", e);
            return JNI_FALSE;
        }
    };

    if buffer_capacity < 0 {
        error!("Invalid buffer capacity: {}", buffer_capacity);
        return JNI_FALSE;
    }

    // Copy buffer immediately to avoid use-after-free (R5)
    // Java GC could move/free the buffer while we're using it
    let buffer_copy = {
        let buffer_slice = unsafe {
            std::slice::from_raw_parts(buffer_ptr, buffer_capacity as usize)
        };
        buffer_slice.to_vec()
    };
    // JNI buffer reference no longer needed after this point

    if let Some(server_container) = VNC_SERVER.get() {
        if let Ok(guard) = server_container.lock() {
            if let Some(server) = guard.as_ref() {
                let rt = get_or_init_vnc_runtime();
                if let Err(e) = rt.block_on(server.framebuffer().update_from_slice(&buffer_copy)) {
                    error!("Failed to update framebuffer: {}", e);
                    return JNI_FALSE;
                }
                // Framebuffer automatically marks itself dirty
                return JNI_TRUE;
            }
        }
    }

    JNI_FALSE
}

/// JNI entry point to update a cropped region of the framebuffer.
///
/// This function is similar to `vncUpdateFramebuffer` but updates only a specified
/// rectangular portion of the screen.
///
/// # Arguments
///
/// * `env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
/// * `buffer` - A Java `DirectByteBuffer` containing the RGBA data for the cropped region.
/// * `_width` - The total width of the framebuffer (unused).
/// * `_height` - The total height of the framebuffer (unused).
/// * `crop_x` - The X coordinate of the top-left corner of the cropped region.
/// * `crop_y` - The Y coordinate of the top-left corner of the cropped region.
/// * `crop_width` - The width of the cropped region.
/// * `crop_height` - The height of the cropped region.
///
/// # Returns
///
/// `JNI_TRUE` if the update is successful, `JNI_FALSE` otherwise.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncUpdateFramebufferCropped(
    env: JNIEnv,
    _class: JClass,
    buffer: JObject,
    _width: jint,
    _height: jint,
    crop_x: jint,
    crop_y: jint,
    crop_width: jint,
    crop_height: jint,
) -> jboolean {
    let buffer_ptr = match env.get_direct_buffer_address((&buffer).into()) {
        Ok(ptr) => ptr,
        Err(e) => {
            error!("Failed to get buffer address: {}", e);
            return JNI_FALSE;
        }
    };

    let buffer_capacity = match env.get_direct_buffer_capacity((&buffer).into()) {
        Ok(cap) => cap,
        Err(e) => {
            error!("Failed to get buffer capacity: {}", e);
            return JNI_FALSE;
        }
    };

    // Validate crop dimensions and calculate size with overflow protection (R3)
    const MAX_DIMENSION: i32 = 8192;

    if crop_x < 0 || crop_y < 0 || crop_width <= 0 || crop_height <= 0 {
        error!("Invalid crop parameters: x={}, y={}, w={}, h={}", crop_x, crop_y, crop_width, crop_height);
        return JNI_FALSE;
    }

    if crop_width > MAX_DIMENSION || crop_height > MAX_DIMENSION {
        error!("Crop dimensions too large: {}x{} (max {})", crop_width, crop_height, MAX_DIMENSION);
        return JNI_FALSE;
    }

    // Use checked multiplication to prevent overflow
    let expected_size = (crop_width as usize)
        .checked_mul(crop_height as usize)
        .and_then(|s| s.checked_mul(4))
        .unwrap_or_else(|| {
            error!("Crop buffer size overflow: {}x{}", crop_width, crop_height);
            0
        });

    if expected_size == 0 || buffer_capacity != expected_size {
        error!(
            "Cropped buffer size mismatch: expected {}, got {}",
            expected_size, buffer_capacity
        );
        return JNI_FALSE;
    }

    // Copy buffer immediately to avoid use-after-free (R5)
    let buffer_copy = {
        let buffer_slice = unsafe {
            std::slice::from_raw_parts(buffer_ptr, buffer_capacity)
        };
        buffer_slice.to_vec()
    };

    if let Some(server_container) = VNC_SERVER.get() {
        if let Ok(guard) = server_container.lock() {
            if let Some(server) = guard.as_ref() {
                let rt = get_or_init_vnc_runtime();
                if let Err(e) = rt.block_on(server.framebuffer().update_cropped(
                    &buffer_copy,
                    crop_x as u16,
                    crop_y as u16,
                    crop_width as u16,
                    crop_height as u16,
                )) {
                    error!("Failed to update cropped framebuffer: {}", e);
                    return JNI_FALSE;
                }
                // Framebuffer automatically marks itself dirty
                return JNI_TRUE;
            }
        }
    }

    JNI_FALSE
}

/// JNI entry point to resize the framebuffer to new dimensions.
///
/// This function creates a new framebuffer with the specified dimensions, preserving
/// as much of the existing content as possible. This is equivalent to libvncserver's
/// `rfbNewFramebuffer` function.
///
/// # Arguments
///
/// * `_env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
/// * `width` - The new width of the framebuffer.
/// * `height` - The new height of the framebuffer.
///
/// # Returns
///
/// `JNI_TRUE` if the resize is successful, `JNI_FALSE` otherwise.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncNewFramebuffer(
    _env: JNIEnv,
    _class: JClass,
    width: jint,
    height: jint,
) -> jboolean {
    // Validate dimensions
    const MAX_DIMENSION: i32 = 8192;
    const MIN_DIMENSION: i32 = 1;

    let width = match u16::try_from(width) {
        Ok(w) if w >= MIN_DIMENSION as u16 && w <= MAX_DIMENSION as u16 => w,
        _ => {
            error!("Invalid width: {} (must be {}-{})", width, MIN_DIMENSION, MAX_DIMENSION);
            return JNI_FALSE;
        }
    };

    let height = match u16::try_from(height) {
        Ok(h) if h >= MIN_DIMENSION as u16 && h <= MAX_DIMENSION as u16 => h,
        _ => {
            error!("Invalid height: {} (must be {}-{})", height, MIN_DIMENSION, MAX_DIMENSION);
            return JNI_FALSE;
        }
    };

    info!("Resizing framebuffer to {}x{}", width, height);

    if let Some(server_container) = VNC_SERVER.get() {
        if let Ok(guard) = server_container.lock() {
            if let Some(server) = guard.as_ref() {
                let runtime = get_or_init_vnc_runtime();

                // Call the resize method on the framebuffer
                // The resize method uses interior mutability (atomic width/height)
                if let Err(e) = runtime.block_on(server.framebuffer().resize(width, height)) {
                    error!("Failed to resize framebuffer: {}", e);
                    return JNI_FALSE;
                }

                info!("Framebuffer resized successfully to {}x{}", width, height);
                return JNI_TRUE;
            }
        }
    }

    error!("VNC server not initialized");
    JNI_FALSE
}

/// JNI entry point to send cut text (clipboard) to all connected VNC clients.
///
/// # Arguments
///
/// * `env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
/// * `text` - A Java `String` containing the text to be sent.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncSendCutText(
    mut env: JNIEnv,
    _class: JClass,
    text: JString,
) {
    let text_str: String = match env.get_string(&text) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get cut text: {}", e);
            return;
        }
    };

    if let Some(server_container) = VNC_SERVER.get() {
        if let Ok(guard) = server_container.lock() {
            if let Some(server) = guard.as_ref() {
                let runtime = get_or_init_vnc_runtime();
                let server_clone = server.clone();
                let text_clone = text_str.clone();

                runtime.spawn(async move {
                    if let Err(e) = server_clone.send_cut_text_to_all(text_clone).await {
                        error!("Failed to send cut text: {}", e);
                    }
                });
            }
        }
    }
}

/// JNI entry point to check if the VNC server is currently active.
///
/// # Arguments
///
/// * `_env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
///
/// # Returns
///
/// `JNI_TRUE` if the server is running, `JNI_FALSE` otherwise.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncIsActive(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    if let Some(server_container) = VNC_SERVER.get() {
        if let Ok(guard) = server_container.lock() {
            if guard.is_some() {
                return JNI_TRUE;
            }
        }
    }
    JNI_FALSE
}

/// JNI entry point to get the current framebuffer width.
///
/// # Arguments
///
/// * `_env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
///
/// # Returns
///
/// The framebuffer width in pixels, or -1 if the server is not active.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncGetFramebufferWidth(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    if let Some(server_container) = VNC_SERVER.get() {
        if let Ok(guard) = server_container.lock() {
            if let Some(server) = guard.as_ref() {
                return server.framebuffer().width() as jint;
            }
        }
    }
    -1
}

/// JNI entry point to get the current framebuffer height.
///
/// # Arguments
///
/// * `_env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
///
/// # Returns
///
/// The framebuffer height in pixels, or -1 if the server is not active.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncGetFramebufferHeight(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    if let Some(server_container) = VNC_SERVER.get() {
        if let Ok(guard) = server_container.lock() {
            if let Some(server) = guard.as_ref() {
                return server.framebuffer().height() as jint;
            }
        }
    }
    -1
}

/// JNI entry point to initiate a direct reverse VNC connection to a viewer.
///
/// This function establishes a direct connection to a VNC viewer without using
/// a repeater. The function blocks until the connection attempt succeeds or fails.
///
/// # Arguments
///
/// * `env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
/// * `host` - The hostname or IP address of the VNC viewer.
/// * `port` - The port on which the VNC viewer is listening.
///
/// # Returns
///
/// The client ID (`jlong`) of the new reverse connection if successful, or `0` on failure.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncConnectReverse(
    mut env: JNIEnv,
    _class: JClass,
    host: JString,
    port: jint,
) -> jlong {
    let host_str: String = match env.get_string(&host) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get reverse connection host: {}", e);
            return 0;
        }
    };

    let port_u16 = port as u16;

    info!(
        "Initiating reverse connection to {}:{}",
        host_str, port_u16
    );

    if let Some(server_container) = VNC_SERVER.get() {
        let server = match server_container.lock() {
            Ok(guard) => {
                if let Some(s) = guard.as_ref() {
                    s.clone()
                } else {
                    error!("VNC server not started");
                    return 0;
                }
            }
            Err(e) => {
                error!("Failed to lock server container: {}", e);
                return 0;
            }
        };

        let runtime = get_or_init_vnc_runtime();

        // Block until connection succeeds or fails
        let result = runtime.block_on(async move {
            match server.connect_reverse(host_str, port_u16).await {
                Ok(client_id) => {
                    info!("Reverse connection established, client ID: {}", client_id);
                    client_id as jlong
                }
                Err(e) => {
                    error!("Failed to establish reverse connection: {}", e);
                    0
                }
            }
        });

        return result;
    }

    error!("VNC server not initialized");
    0
}

/// JNI entry point to connect to a VNC repeater for a reverse connection.
///
/// This function blocks until the connection attempt succeeds or fails.
///
/// # Arguments
///
/// * `env` - The JNI environment.
/// * `_class` - The Java class from which this method is called.
/// * `host` - The hostname or IP address of the VNC repeater.
/// * `port` - The port of the VNC repeater.
/// * `repeater_id` - The ID to use when connecting to the repeater.
///
/// # Returns
///
/// The client ID (`jlong`) of the new repeater connection if successful, or `0` on failure.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_christianbeier_droidvnc_1ng_MainService_vncConnectRepeater(
    mut env: JNIEnv,
    _class: JClass,
    host: JString,
    port: jint,
    repeater_id: JString,
) -> jlong {
    let host_str: String = match env.get_string(&host) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get repeater host: {}", e);
            return 0;
        }
    };

    let repeater_id_str: String = match env.get_string(&repeater_id) {
        Ok(s) => s.into(),
        Err(e) => {
            error!("Failed to get repeater ID: {}", e);
            return 0;
        }
    };

    let port_u16 = port as u16;

    info!(
        "Connecting to VNC repeater {}:{} with ID: {}",
        host_str, port_u16, repeater_id_str
    );

    if let Some(server_container) = VNC_SERVER.get() {
        let server = match server_container.lock() {
            Ok(guard) => {
                if let Some(s) = guard.as_ref() {
                    s.clone()
                } else {
                    error!("VNC server not started");
                    return 0;
                }
            }
            Err(e) => {
                error!("Failed to lock server container: {}", e);
                return 0;
            }
        };

        let runtime = get_or_init_vnc_runtime();

        // Block until connection succeeds or fails
        let result = runtime.block_on(async move {
            match server.connect_repeater(host_str, port_u16, repeater_id_str).await {
                Ok(client_id) => {
                    info!("Repeater connection established, client ID: {}", client_id);
                    client_id as jlong
                }
                Err(e) => {
                    error!("Failed to connect to repeater: {}", e);
                    0
                }
            }
        });

        return result;
    }

    error!("VNC server not initialized");
    0
}

/// Spawns a long-running asynchronous task to handle VNC server events.
///
/// This function starts a single, global event handler that receives `ServerEvent`s
/// from the VNC server and forwards them to the appropriate Java methods. It ensures
/// that only one event handler task is running at any given time.
///
/// # Arguments
///
/// * `event_rx` - An `mpsc::UnboundedReceiver<ServerEvent>` from which to receive server events.
fn spawn_event_handler(mut event_rx: mpsc::UnboundedReceiver<ServerEvent>) {
    // Ensure only one event handler is running at a time using atomic compare-and-swap
    if EVENT_HANDLER_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        warn!("Event handler already running");
        return;
    }

    let runtime = get_or_init_vnc_runtime();
    let mut shutdown_rx = get_or_init_shutdown_signal().subscribe();

    runtime.spawn(async move {
        info!("VNC event handler started");

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    handle_server_event(event);
                }
                _ = shutdown_rx.recv() => {
                    info!("Event handler received shutdown signal");
                    break;
                }
            }
        }

        EVENT_HANDLER_RUNNING.store(false, Ordering::SeqCst);
        info!("VNC event handler stopped");
    });
}

/// Handles a single `ServerEvent`, calling the appropriate Java static method.
///
/// This function attaches the current thread to the Java VM, determines the type of
/// server event, and then makes a JNI call to the corresponding Java method in either
/// MainService or InputService to notify the Android application of the event.
///
/// # Arguments
///
/// * `event` - The `ServerEvent` to handle.
fn handle_server_event(event: ServerEvent) {
    let vm = match JAVA_VM.get() {
        Some(vm) => vm,
        None => {
            error!("Java VM not available");
            return;
        }
    };

    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(e) => {
            error!("Failed to attach to Java thread: {}", e);
            return;
        }
    };

    match event {
        ServerEvent::ClientConnected { client_id } => {
            info!("Client {} connected", client_id);
            if let Some(main_class) = MAIN_SERVICE_CLASS.get() {
                let args = [JValue::Long(client_id as jlong)];
                // Log JNI errors to aid debugging
                if let Err(e) = env.call_static_method(
                    main_class,
                    "onClientConnected",
                    "(J)V",
                    &args,
                ) {
                    error!("Failed to call onClientConnected: {}", e);
                }
            }
        }
        ServerEvent::ClientDisconnected { client_id } => {
            info!("Client {} disconnected", client_id);
            if let Some(main_class) = MAIN_SERVICE_CLASS.get() {
                let args = [JValue::Long(client_id as jlong)];
                // Log JNI errors to aid debugging
                if let Err(e) = env.call_static_method(
                    main_class,
                    "onClientDisconnected",
                    "(J)V",
                    &args,
                ) {
                    error!("Failed to call onClientDisconnected: {}", e);
                }
            }
        }
        ServerEvent::KeyPress { client_id, down, key } => {
            if let Some(input_class) = INPUT_SERVICE_CLASS.get() {
                let args = [
                    JValue::Int(if down { 1 } else { 0 }),
                    JValue::Long(key as jlong),
                    JValue::Long(client_id as jlong),
                ];
                // Log JNI errors to aid debugging
                if let Err(e) = env.call_static_method(
                    input_class,
                    "onKeyEvent",
                    "(IJJ)V",
                    &args,
                ) {
                    error!("Failed to call onKeyEvent: {}", e);
                }
            }
        }
        ServerEvent::PointerMove {
            client_id,
            x,
            y,
            button_mask,
        } => {
            if let Some(input_class) = INPUT_SERVICE_CLASS.get() {
                let args = [
                    JValue::Int(button_mask as jint),
                    JValue::Int(x as jint),
                    JValue::Int(y as jint),
                    JValue::Long(client_id as jlong),
                ];
                // Log JNI errors to aid debugging
                if let Err(e) = env.call_static_method(
                    input_class,
                    "onPointerEvent",
                    "(IIIJ)V",
                    &args,
                ) {
                    error!("Failed to call onPointerEvent: {}", e);
                }
            }
        }
        ServerEvent::CutText { client_id, text } => {
            if let Some(input_class) = INPUT_SERVICE_CLASS.get() {
                if let Ok(jtext) = env.new_string(&text) {
                    let args = [
                        JValue::Object(&jtext),
                        JValue::Long(client_id as jlong),
                    ];
                    // Log JNI errors to aid debugging
                    if let Err(e) = env.call_static_method(
                        input_class,
                        "onCutText",
                        "(Ljava/lang/String;J)V",
                        &args,
                    ) {
                        error!("Failed to call onCutText: {}", e);
                    }
                }
            }
        }
    }
}

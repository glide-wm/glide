// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Safe wrapper around CFMessagePort for inter-process communication.
//!
//! ## Example
//!
//! Creating a local port to receive messages:
//!
//! ```no_run
//! # use objc2_core_foundation::CFRunLoop;
//! # use glide_wm::sys::message_port::LocalMessagePort;
//!
//! let port = LocalMessagePort::new("com.example.myapp.port", |msg_id, data| {
//!     println!("Received message {} with {} bytes", msg_id, data.len());
//!     b"reply".to_vec()
//! })?;
//!
//! CFRunLoop::run();
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Sending messages to a remote port:
//!
//! ```no_run
//! # use glide_wm::sys::message_port::RemoteMessagePort;
//! # use std::time::Duration;
//!
//! let remote = RemoteMessagePort::new("com.example.myapp.port")?;
//! let reply = remote.send_message(1, b"hello", Duration::from_secs(5))?;
//! println!("Received reply: {:?}", String::from_utf8_lossy(&reply));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::mem::ManuallyDrop;
use std::ptr;
use std::time::Duration;
use std::{ffi::c_void, ptr::NonNull};

use objc2_core_foundation::{
    CFData, CFMessagePort, CFMessagePortContext, CFRetained, CFRunLoop, CFString,
    kCFMessagePortIsInvalid, kCFMessagePortReceiveTimeout, kCFMessagePortSendTimeout,
    kCFMessagePortSuccess, kCFMessagePortTransportError, kCFRunLoopCommonModes,
    kCFRunLoopDefaultMode,
};

/// Errors that can occur when creating message ports.
#[derive(Debug)]
pub enum LocalPortCreateError {
    /// Failed to create a local message port
    Failed,

    /// Local port name already exists.
    ///
    /// The raw CFMessagePort is returned.
    AlreadyExists(CFRetained<CFMessagePort>),
}

impl std::fmt::Display for LocalPortCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed => write!(f, "Failed to create local message port"),
            Self::AlreadyExists(_) => write!(f, "Port name already exists"),
        }
    }
}

impl std::error::Error for LocalPortCreateError {}

/// Errors that can occur when creating message ports.
#[derive(Debug)]
pub struct RemotePortCreateError;

impl std::fmt::Display for RemotePortCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Remote message port not found, or creation failed")
    }
}

impl std::error::Error for RemotePortCreateError {}

/// Errors that can occur when sending messages through message ports.
#[derive(Debug)]
pub enum SendError {
    /// The message port is invalid or has been invalidated
    InvalidPort,
    /// Message sending failed
    SendFailed(i32),
    /// Message sending timed out
    Timeout,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPort => write!(f, "Message port is invalid"),
            Self::SendFailed(code) => write!(f, "Message send failed with code {}", code),
            Self::Timeout => write!(f, "Message send timed out"),
        }
    }
}

impl std::error::Error for SendError {}

/// A local message port that can receive messages from other processes.
///
/// This type is not Send as it's tied to the current thread's run loop.
pub struct LocalMessagePort {
    callback: *mut (),
    dtor: unsafe fn(*mut ()),
    port: ManuallyDrop<CFRetained<CFMessagePort>>,
}

static_assertions::assert_not_impl_any!(LocalMessagePort: Send);

impl LocalMessagePort {
    /// Creates a new local message port with the given name and callback.
    ///
    /// The name should be a reverse-DNS style identifier that uniquely
    /// identifies this port system-wide.
    pub fn new<F>(name: &str, callback: F) -> Result<Self, LocalPortCreateError>
    where
        F: Fn(i32, &[u8]) -> Vec<u8> + 'static,
    {
        let cf_name = CFString::from_str(name);

        // Box the callback and get raw pointer
        let callback_ptr = Box::into_raw(Box::new(callback));

        // Create context with the callback
        let mut context = CFMessagePortContext {
            version: 0,
            info: callback_ptr as *mut c_void,
            retain: None,
            release: None,
            copyDescription: None,
        };

        // Create port with context
        let mut should_free_info = 0u8;
        let port = unsafe {
            CFMessagePort::new_local(
                None,
                Some(&cf_name),
                Some(internal_callback::<F>),
                &mut context,
                &mut should_free_info,
            )
        }
        .ok_or_else(|| LocalPortCreateError::Failed)?;

        if should_free_info != 0 {
            // Per docs, this only happens on success if the port already exists.
            drop(unsafe { Box::from_raw(callback_ptr) });
            return Err(LocalPortCreateError::AlreadyExists(port));
        }

        // Add to run loop
        if let Some(source) = CFMessagePort::new_run_loop_source(None, Some(&port), 0) {
            if let Some(current_loop) = CFRunLoop::current() {
                current_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
            }
        }

        Ok(LocalMessagePort {
            callback: callback_ptr as *mut (),
            dtor: destruct::<F>,
            port: ManuallyDrop::new(port),
        })
    }

    /// Returns the name of this message port.
    pub fn name(&self) -> Option<String> {
        self.port.name().map(|s| s.to_string())
    }

    /// Returns whether this message port is still valid.
    pub fn is_valid(&self) -> bool {
        self.port.is_valid()
    }

    /// Invalidates the message port, preventing it from receiving new messages.
    pub fn invalidate(&self) {
        self.port.invalidate()
    }
}

unsafe fn destruct<T>(ptr: *mut ()) {
    let _ = unsafe { Box::from_raw(ptr as *mut T) };
}

impl Drop for LocalMessagePort {
    fn drop(&mut self) {
        unsafe {
            // Invalidate the port first to prevent any new callbacks.
            self.port.invalidate();
            // Drop the callback, then the port.
            (self.dtor)(self.callback);
            ManuallyDrop::drop(&mut self.port);
        }
    }
}

/// A remote message port for sending messages to other processes.
pub struct RemoteMessagePort {
    port: CFRetained<CFMessagePort>,
}

impl RemoteMessagePort {
    /// Creates a connection to a remote message port with the given name.
    ///
    /// Returns an error if no port with that name exists or if the connection fails.
    pub fn new(name: &str) -> Result<Self, RemotePortCreateError> {
        let cf_name = CFString::from_str(name);

        let port = CFMessagePort::new_remote(None, Some(&cf_name)).ok_or(RemotePortCreateError)?;

        Ok(Self { port })
    }

    /// Sends a message to the remote port and waits for a reply.
    ///
    /// NOTE: The run loop is run in the default mode while waiting for a reply.
    ///
    /// # Parameters
    /// - `msg_id`: An application-defined message identifier
    /// - `data`: The message data to send
    /// - `timeout`: How long to wait for the send operation and optional reply
    ///
    /// Returns the reply data on success.
    pub fn send_message(
        &self,
        msg_id: i32,
        data: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, SendError> {
        if !self.is_valid() {
            return Err(SendError::InvalidPort);
        }

        let cf_data = CFData::from_bytes(data);
        let timeout_secs = timeout.as_secs_f64();
        let mut return_data: *const CFData = ptr::null();

        let result = unsafe {
            self.port.send_request(
                msg_id,
                Some(&cf_data),
                timeout_secs,
                timeout_secs,
                // TODO: Expose a no-wait API that sets this to null.
                kCFRunLoopDefaultMode,
                &mut return_data,
            )
        };

        #[allow(non_upper_case_globals)]
        match result {
            kCFMessagePortSuccess => {
                let reply_data = match NonNull::new(return_data as *mut CFData) {
                    Some(data) => unsafe { CFRetained::retain(data) }.to_vec(),
                    None => Vec::new(),
                };
                Ok(reply_data)
            }
            kCFMessagePortSendTimeout | kCFMessagePortReceiveTimeout => Err(SendError::Timeout),
            kCFMessagePortIsInvalid => Err(SendError::InvalidPort),
            kCFMessagePortTransportError => Err(SendError::SendFailed(result)),
            code => Err(SendError::SendFailed(code)),
        }
    }

    /// Returns whether this remote message port connection is still valid.
    pub fn is_valid(&self) -> bool {
        self.port.is_valid()
    }

    /// Returns the name of the remote message port.
    pub fn name(&self) -> Option<String> {
        self.port.name().map(|s| s.to_string())
    }
}

/// Internal callback function that bridges from C to Rust.
unsafe extern "C-unwind" fn internal_callback<F>(
    _port: *mut CFMessagePort,
    msg_id: i32,
    data: *const CFData,
    info: *mut c_void,
) -> *const CFData
where
    F: Fn(i32, &[u8]) -> Vec<u8> + 'static,
{
    if info.is_null() || data.is_null() {
        return ptr::null();
    }

    // SAFETY: This is the original type we cast from.
    let callback = unsafe { &*(info as *const F) };

    // SAFETY: We assume the system gives us a valid pointer, and we check for null above.
    let cf_data = unsafe { CFRetained::retain(NonNull::new_unchecked(data as *mut CFData)) };
    // SAFETY: cf_data is not mutated for the lifetime of the callback.
    let bytes = unsafe { cf_data.as_bytes_unchecked() };

    // Call the user callback
    let reply = callback(msg_id, bytes);
    if reply.is_empty() {
        return ptr::null();
    }
    let reply_cf_data = CFData::from_bytes(&reply);
    CFRetained::into_raw(reply_cf_data).as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn test_local_port_creation() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        let port_name = format!("com.test.glide.message_port_test_1_{}", timestamp);
        let port_name = port_name.as_str();
        let _port = LocalMessagePort::new(port_name, |_msg_id, _data| vec![])
            .expect("Should create local port");
        // Port should be automatically cleaned up on drop
    }

    #[test]
    fn test_remote_port_not_found() {
        let result = RemoteMessagePort::new("com.test.nonexistent.port");
        assert!(matches!(result, Err(RemotePortCreateError)));
    }

    #[test]
    fn test_message_round_trip() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        let port_name = format!("com.test.glide.message_port_test_2_{}", timestamp);
        let port_name = port_name.as_str();

        let received_messages = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received_messages.clone();

        // Create local port that receives messages
        let local_port = LocalMessagePort::new(port_name, move |msg_id, data| {
            received_clone.lock().unwrap().push((msg_id, data.to_vec()));
            // Return echo reply
            format!("echo: {}", String::from_utf8_lossy(data)).into_bytes()
        })
        .expect("Should create local port");

        assert!(local_port.is_valid());

        // Send message from another thread
        let port_name_clone = port_name.to_string();
        let send_thread = thread::spawn(move || {
            let remote =
                RemoteMessagePort::new(&port_name_clone).expect("Should connect to remote port");
            assert!(remote.is_valid());

            let test_message = b"Hello, message port!";
            remote
                .send_message(42, test_message, Duration::from_secs(1))
                .expect("Should send message successfully")
        });

        // Run the CFRunLoop for a limited time to process messages and replies
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, 0.1, false);
            // Continue running until we've processed the message and the send thread completes
            if received_messages.lock().unwrap().len() > 0 && send_thread.is_finished() {
                break;
            }
        }

        let reply = send_thread.join().expect("Send thread should complete");

        // Verify the message was received
        let messages = received_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, 42);
        assert_eq!(messages[0].1, b"Hello, message port!");

        // Test synchronous replies
        let reply_text = String::from_utf8(reply).expect("Reply should be valid UTF-8");
        assert_eq!(reply_text, "echo: Hello, message port!");
    }
}

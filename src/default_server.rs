use std::mem::MaybeUninit;
use std::net::TcpListener;
use std::sync::{Arc, Once, RwLock};

use crate::server::DebuggableServer;

static mut DEFAULT_DEBUGGABLE_SERVER: MaybeUninit<Arc<RwLock<DebuggableServer>>> = MaybeUninit::uninit();
static DEFAULT_DEBUGGABLE_SERVER_ONCE: Once = Once::new();
static mut DEFAULT_DEBUGGABLE_SERVER_INITIALIZER: fn() -> DebuggableServer = || DebuggableServer::new(TcpListener::bind("127.0.0.1:5050").unwrap());

pub fn default_debuggable_server() -> Arc<RwLock<DebuggableServer>> {
    unsafe {
        DEFAULT_DEBUGGABLE_SERVER_ONCE.call_once(|| {
            let server = DEFAULT_DEBUGGABLE_SERVER_INITIALIZER();
            DEFAULT_DEBUGGABLE_SERVER.write(Arc::new(RwLock::new(server)));
        });
        DEFAULT_DEBUGGABLE_SERVER.assume_init_ref().clone()
    }
}

pub fn set_default_debuggable_server_initializer(initializer: fn() -> DebuggableServer) {
    unsafe { DEFAULT_DEBUGGABLE_SERVER_INITIALIZER = initializer; }
}

pub fn is_default_debuggable_server_initialized() -> bool {
    DEFAULT_DEBUGGABLE_SERVER_ONCE.is_completed()
}
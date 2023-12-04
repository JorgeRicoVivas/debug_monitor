use std::mem::MaybeUninit;
use std::net::TcpListener;
use std::sync::{Arc, Once, RwLock};

use crate::server::debuggable_server_builder::DebuggableServerBuilder;
use crate::server::DebuggableServer;

static mut DEFAULT_SERVER: MaybeUninit<Arc<RwLock<DebuggableServer>>> = MaybeUninit::uninit();
static DEFAULT_SERVER_ONCE: Once = Once::new();
static mut DEFAULT_SERVER_INITIALIZER: fn() -> DebuggableServerBuilder = || DebuggableServerBuilder::new(TcpListener::bind("127.0.0.1:5050").unwrap());

pub fn default_server() -> Arc<RwLock<DebuggableServer>> {
    unsafe {
        DEFAULT_SERVER_ONCE.call_once(|| {
            let server_builder = DEFAULT_SERVER_INITIALIZER();
            DEFAULT_SERVER.write(Arc::new(RwLock::new(server_builder.build())));
        });
        DEFAULT_SERVER.assume_init_ref().clone()
    }
}

pub fn set_default_server_initializer(initializer: fn() -> DebuggableServerBuilder) {
    unsafe { DEFAULT_SERVER_INITIALIZER = initializer; }
}

pub fn is_default_server_initialized() -> bool {
    DEFAULT_SERVER_ONCE.is_completed()
}
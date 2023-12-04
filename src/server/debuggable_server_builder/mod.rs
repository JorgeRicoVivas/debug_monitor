use std::net::TcpListener;

use crate::server::DebuggableServer;

pub struct DebuggableServerBuilder {
    tcp_listener: TcpListener,
    read_dir: Option<String>,
    only_reads_from_dir: bool,
}

impl DebuggableServerBuilder {
    pub fn new(tcp_listener: TcpListener) -> DebuggableServerBuilder {
        Self {
            tcp_listener,
            read_dir: None,
            only_reads_from_dir: false,
        }
    }

    pub fn only_reads_from_dir(mut self) -> Self {
        self.only_reads_from_dir = true;
        self
    }

    pub fn read_dir<ReadDir: ToString>(mut self, read_dir: ReadDir) -> Self {
        self.read_dir = Some(read_dir.to_string());
        self
    }

    pub fn build(self) -> DebuggableServer {
        let mut server = DebuggableServer::new(self.tcp_listener);
        server.set_read_dir(self.read_dir);
        if self.only_reads_from_dir {
            server.set_only_reads_from_dir(true);
        }
        server
    }
}

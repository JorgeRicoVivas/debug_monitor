use std::{fs, mem};
use std::cmp::Ordering;
use std::collections::{HashSet, VecDeque};
use std::fs::metadata;
use std::net::TcpListener;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use fixed_index_vec::fixed_index_vec::FixedIndexVec;
use simple_tcp::server::SimpleServer;

use crate::serializable::JSONDeSerializable;
use crate::serializable::messages::{ClientUnitMessage, ServerMessage};

pub mod debuggable_server_builder;

#[derive(Debug)]
pub struct DebuggableServer(SimpleServer<DebuggableServerData, ()>);

impl Deref for DebuggableServer {
    type Target = SimpleServer<DebuggableServerData, ()>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DebuggableServer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct DebuggableServerData {
    debuggables: FixedIndexVec<DebuggableOnServer>,
    only_reads_from_dir: bool,
    read_from_dir: Option<String>,
}

impl DebuggableServer {
    pub fn new(tcp_listener: TcpListener) -> DebuggableServer {
        let mut server = Self {
            0: SimpleServer::new(tcp_listener, DebuggableServerData {
                debuggables: FixedIndexVec::new(),
                only_reads_from_dir: false,
                read_from_dir: None,
            },
                                 |_, _, _| None)
        };
        server.on_request_accept(|_, _, _| Some(()));
        server.on_accept(|server, client_index| {
            Self::init_client(server, client_index);
        });
        server.on_get_message(|server, client_id, message| {
            Self::process_message_of(server, client_id, message)
        });
        server.on_close(|server| {
            server.send_message_to_all_clients(&*ServerMessage::RemoveAll.to_json().unwrap());
        });
        server
    }

    fn init_client(server: &mut SimpleServer<DebuggableServerData, ()>, client_index: &usize) {
        server.send_message_to_client(*client_index,
                                      &*ServerMessage::GiveClientId { client_id: *client_index }.to_json().unwrap());
        for debuggable_index in 0..server.debuggables.len() {
            let debuggable = server.debuggables.get(debuggable_index);
            if debuggable.is_none() { continue; }
            let debuggable = debuggable.unwrap();
            let notify_value_message = &*ServerMessage::Notify {
                name: debuggable.name.clone(),
                id: debuggable_index,
                value_in_json: debuggable.last_value.clone().unwrap_or_else(|| "{}".to_string()),
            }.to_json().unwrap();
            server.send_message_to_client(*client_index, notify_value_message);
        }
    }

    fn process_message_of(server: &mut SimpleServer<DebuggableServerData, ()>, client_id: &usize, message: &str) {
        let client_unit_message = ClientUnitMessage::from_json(message);
        if client_unit_message.is_none() {
            println!("Got wrong message {}", message);
            println!("Got wrong message as debug {:?}", message);
            return;
        };
        let client_message = client_unit_message.unwrap();
        println!("Got client message {client_message:?}");
        match client_message {
            ClientUnitMessage::UpdateValue { id, new_value } => {
                println!("Got update value");
                let debuggable = server.debuggables.get_mut(id);
                if debuggable.is_none() { return; }
                debuggable.unwrap().incoming_jsons.push((*client_id, new_value));
            }
            ClientUnitMessage::Renotify => {
                println!("Got renotify");
                if server.get_client(*client_id).is_some() {
                    Self::init_client(server, client_id);
                } else {
                    let clients_to_notify = (0..server.clients_len())
                        .into_iter()
                        .filter(|client_id| server.get_client(*client_id).is_some())
                        .collect::<Vec<_>>();
                    clients_to_notify.into_iter().for_each(|client| Self::init_client(server, client_id));
                }
            }
        }
    }

    pub fn set_read_dir(&mut self, read_dir: Option<String>) {
        self.read_from_dir = read_dir;
    }

    pub fn set_only_reads_from_dir(&mut self, only_reads_from_dir: bool) {
        self.only_reads_from_dir = only_reads_from_dir;
    }

    pub fn read_all_clients(&mut self) -> usize {
        if self.only_reads_from_dir {
            return self.read_clients_from_read_dir();
        }
        let read_bytes: usize = 0;
        read_bytes.checked_add(self.read_clients(true)).unwrap_or(usize::MAX);
        read_bytes.checked_add(self.read_clients_from_read_dir()).unwrap_or(usize::MAX);
        read_bytes
    }

    pub fn read_clients_from_read_dir(&mut self) -> usize {
        let mut read_bytes = 0_usize;
        if self.read_from_dir.is_none() { return read_bytes; }
        let dir_read = fs::read_dir(self.read_from_dir.as_ref().unwrap());
        if dir_read.is_err() { return read_bytes; }
        let dir_read = dir_read.unwrap();
        let mut transactions = dir_read.into_iter()
            .filter(Result::is_ok)
            .map(Result::unwrap)
            .filter(|file| metadata(file.path()).map(|metada| metada.is_file()).unwrap_or(false))
            .map(|file| {
                let file_name = file.file_name().to_os_string().into_string();
                (file, file_name)
            })
            .filter(|(_, file_name)| file_name.is_ok())
            .map(|(file, filename)| (file, filename.unwrap()))
            .map(|(file, filename)| {
                let contains_names = filename.contains("client-") && filename.contains("-transaction-");
                if !contains_names { return None; }
                let delimited_name = filename.replace("client-", "").replace("-transaction-", "-");
                let ids = delimited_name.split("-").map(usize::from_str).collect::<Vec<_>>();
                let are_valid_ids = ids.len() == 2 && ids.iter().all(Result::is_ok);
                if !are_valid_ids { return None; }
                let mut ids = ids.into_iter().map(Result::unwrap).collect::<VecDeque<_>>();
                Some((file, ids.pop_front().unwrap(), ids.pop_front().unwrap()))
            })
            .filter(Option::is_some)
            .map(Option::unwrap)
            .map(|(file, client_id, transaction)|
                ((file, client_id, transaction)))
            .map(|(file, client_id, transaction)| {
                let contents = fs::read_to_string(file.path());
                (file, client_id, transaction, contents)
            })
            .filter(|(_, _, _, contents)| contents.is_ok())
            .map(|(file, client_id, transaction, contents)|
                ((file, client_id, transaction, contents.unwrap()))
            )
            .filter(|(file, _, _, _)| fs::remove_file(file.path()).is_ok())
            .map(|(_, client_id, transaction, contents)| {
                (client_id, transaction, contents)
            })
            .collect::<Vec<_>>();
        transactions.sort_by(|(this_client_id, this_transaction, _), (other_client_id, other_transaction, _)| {
            if this_client_id != other_client_id {
                Ordering::Equal
            } else {
                this_transaction.cmp(other_transaction)
            }
        });
        transactions.into_iter().for_each(|(client_id, _, contents)| {
            read_bytes = read_bytes.checked_add(contents.len()).unwrap_or(usize::MAX);
            let contents = &*contents.replace(self.0.endmark().escape(), self.0.endmark().string());
            Self::process_message_of(self, &client_id, contents);
        });
        read_bytes
    }

    pub(crate) fn notify_new_value(&mut self, changed_id: usize, changed_value: Option<String>, who: Who) {
        self.debuggables.get_mut(changed_id).unwrap().last_value = changed_value;
        let value = self.debuggables.get(changed_id).unwrap();
        let clients_to_notify: Vec<usize> = match who {
            Who::Client(client_id) => vec![client_id],
            Who::All => (0..self.clients_len()).into_iter().collect(),
            Who::AllBut(except_client) => {
                let mut clients_to_notify = (0..self.clients_len()).into_iter().collect::<Vec<_>>();
                clients_to_notify.remove(except_client);
                clients_to_notify
            }
            Who::WrongClients(wrong_clients) => {
                wrong_clients.into_iter().collect()
            }
        };
        let notify_value_message = &*ServerMessage::Notify {
            id: changed_id,
            name: value.name.clone(),
            value_in_json: value.last_value.as_ref().unwrap_or(&"{}".to_string()).clone(),
        }.to_json().unwrap();
        self.send_message_to_clients(&*clients_to_notify, notify_value_message);
    }

    pub(crate) fn init_debuggable(&mut self, name: String) -> usize {
        self.debuggables.push(DebuggableOnServer::new(name, None, Vec::new()))
    }

    pub(crate) fn last_value_of_equals(&self, debuggable_id: usize, current_value: &Option<String>) -> bool {
        self.debuggables.get(debuggable_id).unwrap().last_value.eq(current_value)
    }

    pub(crate) fn take_incoming_jsons_of(&mut self, debuggable_id: usize) -> Vec<(usize, String)> {
        mem::take(&mut self.debuggables.get_mut(debuggable_id).unwrap().incoming_jsons)
    }
}

#[derive(Debug)]
pub(crate) struct DebuggableOnServer {
    name: String,
    last_value: Option<String>,
    incoming_jsons: Vec<(usize, String)>,
}

impl DebuggableOnServer {
    pub fn new(name: String, last_value: Option<String>, incoming_jsons: Vec<(usize, String)>) -> Self {
        Self { name, last_value, incoming_jsons }
    }
}

pub(crate) enum Who {
    Client(usize),
    All,
    AllBut(usize),
    WrongClients(HashSet<usize>),
}
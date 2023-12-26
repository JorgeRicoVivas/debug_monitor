use std::{fs, mem};
use std::cmp::Ordering;
use std::collections::{HashSet, VecDeque};
use std::fs::metadata;
use std::net::TcpListener;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use fixed_index_vec::fixed_index_vec::FixedIndexVec;
use simple_tcp::server::Server;
use simple_tcp::simple_server::{InnerSimpleServer, SimpleServer};
use simple_tcp::simple_server::builder::SimpleServerBuilder;
use simple_tcp::unchecked_read_write_lock::UncheckedRwLock;

use crate::serializable::{ClientUnitMessage, JSONDeSerializable, ServerMessage};

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
        let server = SimpleServerBuilder::new(tcp_listener,
                                              DebuggableServerData {
                                                  debuggables: FixedIndexVec::new(),
                                                  only_reads_from_dir: false,
                                                  read_from_dir: None,
                                              }, |_, _, _| Some(()))
            .on_accept(|server, client_index| {
                Self::init_client(server, client_index);
            })
            .on_get_message(|server, client_id, message| {
                Self::process_message_of(server, client_id, message)
            })
            .on_close(|server| {
                let remove_all_debuggables_message = &*ServerMessage::RemoveAll.to_json().unwrap();
                (0..server.read().clients().len()).into_iter().for_each(|client_index| {
                    server.send_message_to_client(client_index, remove_all_debuggables_message);
                })
            })
            .build();
        Self { 0: server }
    }

    fn init_client(server: &UncheckedRwLock<InnerSimpleServer<DebuggableServerData, ()>>, client_index: usize) {
        server.read().send_message_to_client(client_index, &*ServerMessage::GiveClientId { client_id: client_index }.to_json().unwrap());
        for (debuggable_index, debuggable) in server.read().debuggables.iter_index() {
            let notify_value_message = &*ServerMessage::Notify {
                name: debuggable.name.clone(),
                id: debuggable_index,
                value_in_json: debuggable.last_value.clone().unwrap_or_else(|| "{}".to_string()),
            }.to_json().unwrap();
            server.read().send_message_to_client(client_index, notify_value_message);
        }
    }

    fn process_message_of(server: &UncheckedRwLock<InnerSimpleServer<DebuggableServerData, ()>>, client_id: usize, message: String) {
        let client_unit_message = ClientUnitMessage::from_json(&message);
        if client_unit_message.is_none() {
            return;
        };
        let client_message = client_unit_message.unwrap();
        match client_message {
            ClientUnitMessage::UpdateValue { id, new_value } => {
                match server.write().debuggables.get_mut(id) {
                    None => { return; }
                    Some(debuggable) => {
                        debuggable.incoming_jsons.push((client_id, new_value));
                    }
                }
            }
            ClientUnitMessage::Renotify => {
                if server.read().clients().contains_index(client_id) {
                    Self::init_client(server, client_id);
                } else {
                    server.read().clients()
                        .iter_index()
                        .map(|(index, _)| index)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .for_each(|client| Self::init_client(server, client_id));
                }
            }
        }
    }

    pub fn set_read_dir(&mut self, read_dir: Option<String>) {
        self.write().read_from_dir = read_dir;
    }

    pub fn set_only_reads_from_dir(&mut self, only_reads_from_dir: bool) {
        self.write().only_reads_from_dir = only_reads_from_dir;
    }

    pub fn read_all_clients(&self) {
        if self.read().only_reads_from_dir {
            self.read_clients_from_read_dir();
            return;
        }
        self.read_clients_no_context(true);
        self.read_clients_from_read_dir();
    }

    pub fn read_clients_from_read_dir(&self) -> usize {
        let mut read_bytes = 0_usize;
        if self.read().read_from_dir.is_none() { return read_bytes; }
        let dir_read = fs::read_dir(self.read().read_from_dir.as_ref().unwrap());
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
            let server = self.0.read();
            let end_mark = server.message_endmark();
            let contents = contents.replace(end_mark.escape(), end_mark.string());
            drop(server);
            Self::process_message_of(self, client_id, contents);
        });
        read_bytes
    }

    pub(crate) fn notify_new_value(&self, changed_id: usize, changed_value: Option<String>, who: Who) {
        self.write().debuggables.get_mut(changed_id).unwrap().last_value = changed_value;
        let clients_to_notify: Vec<usize> = match who {
            Who::Client(client_id) => vec![client_id],
            Who::All => (0..self.clients_len()).into_iter().collect(),
            Who::AllBut(except_client) => {
                let mut clients_to_notify = (0..self.clients_len()).into_iter().collect::<Vec<_>>();
                if except_client < clients_to_notify.len() {
                    clients_to_notify.remove(except_client);
                }
                clients_to_notify
            }
            Who::WrongClients(wrong_clients) => {
                wrong_clients.into_iter().collect()
            }
        };
        let notify_value_message = &*ServerMessage::Notify {
            id: changed_id,
            name: self.read().debuggables.get(changed_id).unwrap().name.clone(),
            value_in_json: self.read().debuggables.get(changed_id).unwrap().last_value.as_ref().unwrap_or(&"{}".to_string()).clone(),
        }.to_json().unwrap();
        self.send_message_to_clients(&*clients_to_notify, notify_value_message);
    }

    pub(crate) fn init_debuggable(&self, name: String) -> usize {
        self.write().debuggables.push(DebuggableOnServer::new(name, None, Vec::new()))
    }

    pub(crate) fn remove_debuggable(&self, debuggable_id: usize) -> Option<DebuggableOnServer> {
        self.write().debuggables.remove(debuggable_id)
    }

    pub(crate) fn last_value_of_equals(&self, debuggable_id: usize, current_value: &Option<String>) -> bool {
        self.write().debuggables.get(debuggable_id).unwrap().last_value.eq(current_value)
    }

    pub(crate) fn take_incoming_jsons_of(&self, debuggable_id: usize) -> Vec<(usize, String)> {
        mem::take(&mut self.write().debuggables.get_mut(debuggable_id).unwrap().incoming_jsons)
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
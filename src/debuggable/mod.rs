use std::cell::UnsafeCell;
use std::collections::HashSet;
use std::fmt::Debug;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock};

use crate::serializable::JSONDeSerializable;
use crate::serializable::ServerMessage;
use crate::server::{DebuggableServer, Who};
use simple_tcp::server::Server;
use crate::default_server;

pub struct Debuggable<Value> where Value: JSONDeSerializable {
    value: UnsafeCell<Value>,
    id: usize,
    server: Arc<RwLock<DebuggableServer>>,
}

pub struct DebuggableBuilder<Value: JSONDeSerializable> {
    initial_value: Value,
    name: String,
    server: Option<Arc<RwLock<DebuggableServer>>>,
    is_keep: bool,
}


impl<Value: JSONDeSerializable> DebuggableBuilder<Value> {

    pub fn new<Name: ToString>(name: Name, initial_value: Value) -> Self {
        Self { initial_value, name: name.to_string(), server: None, is_keep: false }
    }

    pub fn server(mut self, server: Option<Arc<RwLock<DebuggableServer>>>) -> DebuggableBuilder<Value> {
        self.server = server;
        self
    }

    pub fn keep(mut self) -> DebuggableBuilder<Value> {
        self.is_keep = true;
        self
    }

    pub fn dont_keep(mut self) -> DebuggableBuilder<Value> {
        self.is_keep = false;
        self
    }

    pub fn set_is_keep(mut self, is_keep: bool) -> DebuggableBuilder<Value> {
        self.is_keep = is_keep;
        self
    }

    pub fn build(mut self) -> Debuggable<Value> {
        let server = self.server.unwrap_or_else(|| default_server::default_server());
        Debuggable::new_server(server, self.name, self.initial_value, self.is_keep)
    }
}


impl<Value: JSONDeSerializable> Debuggable<Value> {
    pub fn new<Name: ToString>(name: Name, initial_value: Value) -> Self {
        Self::new_server(default_server::default_server(), name, initial_value, false)
    }

    pub fn new_server<Name: ToString>(server: Arc<RwLock<DebuggableServer>>, name: Name, initial_value: Value, is_keep: bool) -> Self {
        let name = name.to_string();
        let id = server.write().unwrap().init_debuggable(name, is_keep);
        let initial_value = if is_keep {
            server.read().unwrap().last_value_of(id).map(|json| Value::from_json(&json)).flatten().unwrap_or(initial_value)
        } else {
            initial_value
        };
        server.write().unwrap().notify_new_value(id, initial_value.to_json(), Who::All);
        Self { value: UnsafeCell::new(initial_value), id, server }
    }

    fn process_changes(&self) {
        self.server.read().unwrap().accept_incoming_not_blocking();
        self.server.read().unwrap().read_all_clients();
        let current_json = unsafe { (*self.value.get()).to_json() };
        let has_changed = !self.server.read().unwrap().last_value_of_equals(self.id, &current_json);
        let incoming_jsons = self.server.write().unwrap().take_incoming_jsons_of(self.id);
        let mut wrong_clients: HashSet<usize> = HashSet::new();
        let new_value = incoming_jsons.into_iter().rev().map(|(client, new_json)| {
            let json_is_different = current_json.is_none() || new_json.ne(current_json.as_ref().unwrap());
            if !json_is_different { return None; }
            let new_value = Value::from_json(&new_json);
            if new_value.is_none() {
                wrong_clients.insert(client);
                return None;
            }
            let new_value = new_value.unwrap();
            if new_value.to_json().is_none() {
                wrong_clients.insert(client);
                return None;
            }
            Some((client, new_value))
        })
            .next().unwrap_or(None);

        let who_to_notify = if new_value.is_some() {
            Some(Who::AllBut(new_value.as_ref().unwrap().0))
        } else if has_changed {
            Some(Who::All)
        } else if !has_changed && !wrong_clients.is_empty() {
            Some(Who::WrongClients(wrong_clients))
        } else {
            None
        };
        if who_to_notify.is_some() {
            self.server.write().unwrap().notify_new_value(self.id, current_json, who_to_notify.unwrap());
        }
        if new_value.is_none() { return; }
        let (_, new_value) = new_value.unwrap();
        unsafe { *self.value.get() = new_value; }
    }
}

impl<Value: JSONDeSerializable> Deref for Debuggable<Value> {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        unsafe {
            self.process_changes();
            &*self.value.get()
        }
    }
}

impl<Value: JSONDeSerializable> DerefMut for Debuggable<Value> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.process_changes();
        self.value.get_mut()
    }
}

impl<Value: JSONDeSerializable> Drop for Debuggable<Value> {
    fn drop(&mut self) {
        self.server.read().unwrap().remove_debuggable(self.id);
    }
}

impl<Value> Debug for Debuggable<Value> where Value: Debug + JSONDeSerializable {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let _ = self.deref();
        unsafe { f.write_str(&*format!("{:?}", *self.value.get())) }
    }
}
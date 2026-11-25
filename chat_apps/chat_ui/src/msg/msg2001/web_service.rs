use crate::chat_socket::ChatSocket;
use chat_domain::ChatMessage;
use leptos::prelude::*;
use shared::{ClientEvent, ServerEvent};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
pub struct RoomConnection {
    socket: RefCell<Option<ChatSocket>>,
    generation: Cell<u64>,
    alive: Cell<bool>,
}

impl RoomConnection {
    pub fn dispose(&self) {
        self.alive.set(false);
        self.generation.set(self.generation.get().wrapping_add(1));
        self.socket.borrow_mut().take();
    }
    pub fn connect(
        self: &Rc<Self>,
        room: &str,
        messages: WriteSignal<Vec<ChatMessage>>,
        status: WriteSignal<String>,
        failed: RwSignal<bool>,
        busy: RwSignal<bool>,
        history: Callback<()>,
    ) {
        self.socket.borrow_mut().take();
        self.alive.set(true);
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        busy.set(true);
        let location = web_sys::window().unwrap().location();
        let protocol = if location.protocol().unwrap() == "https:" {
            "wss"
        } else {
            "ws"
        };
        let url = format!("{protocol}://{}/ws/{room}", location.host().unwrap());
        let received = Rc::downgrade(self);
        let changed = Rc::downgrade(self);
        let result = ChatSocket::connect(
            &url,
            move |event| {
                if !received
                    .upgrade()
                    .is_some_and(|c| c.alive.get() && c.generation.get() == generation)
                {
                    return;
                }
                if let ServerEvent::MessageCreated(message) = event {
                    messages.update(|rows| {
                        if !rows.iter().any(|r| r.id == message.id) {
                            rows.push(message);
                        }
                    });
                }
            },
            move |text| {
                if !changed
                    .upgrade()
                    .is_some_and(|c| c.alive.get() && c.generation.get() == generation)
                {
                    return;
                }
                status.set(text.into());
                if text == "已連線" {
                    busy.set(false);
                    failed.set(false);
                    history.run(());
                } else if text != "連線中..." {
                    busy.set(false);
                    failed.set(true);
                }
            },
        );
        match result {
            Ok(socket) => *self.socket.borrow_mut() = Some(socket),
            Err(_) => {
                busy.set(false);
                failed.set(true);
                status.set("連線失敗，請重新登入或稍後重試".into());
            }
        }
    }
    pub fn send(&self, event: &ClientEvent) -> Result<(), String> {
        self.socket
            .borrow()
            .as_ref()
            .ok_or_else(|| "尚未連線".to_owned())?
            .send(event)
    }
}

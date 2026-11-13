use shared::ClientEvent;

#[cfg(any(target_arch = "wasm32", test))]
use shared::ServerEvent;

#[cfg(any(target_arch = "wasm32", test))]
pub fn decode_server_event(text: &str) -> Result<ServerEvent, serde_json::Error> {
    serde_json::from_str::<ServerEvent>(text)
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::rc::Rc;

    use wasm_bindgen::{JsCast, closure::Closure};
    use web_sys::{Event, MessageEvent, WebSocket};

    use super::*;

    pub struct ChatSocket {
        socket: WebSocket,
        _onmessage: Closure<dyn FnMut(MessageEvent)>,
        _onopen: Closure<dyn FnMut(Event)>,
        _onclose: Closure<dyn FnMut(Event)>,
        _onerror: Closure<dyn FnMut(Event)>,
    }

    impl ChatSocket {
        pub fn connect<F, S>(url: &str, mut on_event: F, on_status: S) -> Result<Self, String>
        where
            F: 'static + FnMut(ServerEvent),
            S: 'static + Fn(&'static str),
        {
            let socket = WebSocket::new(url)
                .map_err(|err| format!("Failed to create WebSocket: {:?}", err))?;

            let onmessage =
                Closure::wrap(
                    Box::new(move |event: MessageEvent| match event.data().as_string() {
                        Some(text) => match decode_server_event(&text) {
                            Ok(event) => on_event(event),
                            Err(_) => {
                                web_sys::console::error_1(&"Failed to decode server event".into())
                            }
                        },
                        None => return,
                    }) as Box<dyn FnMut(MessageEvent)>,
                );

            let status = Rc::new(on_status);
            status("連線中...");
            let opened = status.clone();
            let closed = status.clone();
            let failed = status.clone();

            let onopen =
                Closure::wrap(Box::new(move |_: Event| opened("已連線")) as Box<dyn FnMut(Event)>);
            let onclose = Closure::wrap(
                Box::new(move |_: Event| closed("已斷線，請重新進房")) as Box<dyn FnMut(Event)>
            );
            let onerror =
                Closure::wrap(Box::new(move |_: Event| failed("連線錯誤，請重新進房"))
                    as Box<dyn FnMut(Event)>);

            socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
            socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));

            Ok(Self {
                socket,
                _onmessage: onmessage,
                _onopen: onopen,
                _onclose: onclose,
                _onerror: onerror,
            })
        }

        pub fn send(&self, event: &ClientEvent) -> Result<(), String> {
            if self.socket.ready_state() != WebSocket::OPEN {
                return Err("尚未連線".into());
            }
            let json = serde_json::to_string(event)
                .map_err(|err| format!("Failed to serialize event: {:?}", err))?;
            self.socket
                .send_with_str(&json)
                .map_err(|err| format!("Failed to send event: {:?}", err))
        }
    }

    impl Drop for ChatSocket {
        fn drop(&mut self) {
            self.socket.set_onmessage(None);
            self.socket.set_onopen(None);
            self.socket.set_onclose(None);
            self.socket.set_onerror(None);
            let _ = self.socket.close();
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::ChatSocket;

#[cfg(not(target_arch = "wasm32"))]
pub struct ChatSocket;

#[cfg(not(target_arch = "wasm32"))]
impl ChatSocket {
    pub fn server_stub() -> Self {
        Self
    }

    pub fn send(&self, _event: &ClientEvent) -> Result<(), String> {
        Err("ChatSocket is not supported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_pong_event() {
        let event =
            decode_server_event(r#"{"type":"pong"}"#).expect("Failed to decode server event");

        assert!(matches!(event, ServerEvent::Pong));
    }
}

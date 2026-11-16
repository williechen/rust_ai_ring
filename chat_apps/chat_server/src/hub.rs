use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use shared::ServerEvent;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct RoomHub {
    rooms: Arc<Mutex<HashMap<String, broadcast::Sender<ServerEvent>>>>,
    capacity: usize,
    members: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
}

impl RoomHub {
    pub fn new(capacity: usize) -> Self {
        Self {
            rooms: Arc::new(Mutex::new(HashMap::new())),
            capacity,
            members: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn subscribe(
        &self,
        room_id: String,
    ) -> (
        broadcast::Sender<ServerEvent>,
        broadcast::Receiver<ServerEvent>,
    ) {
        let mut rooms = self.rooms.lock().unwrap();

        let sender = rooms
            .entry(room_id)
            .or_insert_with(|| {
                let (sender, _receiver) = broadcast::channel(self.capacity);
                sender
            })
            .clone();

        let receiver = sender.subscribe();

        (sender, receiver)
    }

    pub fn subscriber_count(&self, room_id: &str) -> usize {
        self.rooms
            .lock()
            .unwrap()
            .get(room_id)
            .map_or(0, |sender| sender.receiver_count())
    }

    pub fn publish(&self, room_id: String, event: ServerEvent) {
        let sender = {
            let rooms = self.rooms.lock().unwrap();
            rooms.get(&room_id).cloned()
        };
        if let Some(sender) = sender {
            let _ = sender.send(event);
        }
    }

    pub fn cleanup(&self, room_id: String, sender: &broadcast::Sender<ServerEvent>) {
        let mut rooms = self.rooms.lock().unwrap();
        let should_remove = rooms.get(&room_id).is_some_and(|current_sender| {
            current_sender.same_channel(sender) && current_sender.receiver_count() == 0
        });

        if should_remove {
            rooms.remove(&room_id);
        }
    }
}

/// 連線離開或工作取消時，移除這一份會員在場計數。
pub struct MemberPresence {
    members: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
    room: String,
    member: String,
}
impl Drop for MemberPresence {
    fn drop(&mut self) {
        let mut rooms = self.members.lock().unwrap();
        if let Some(members) = rooms.get_mut(&self.room) {
            if let Some(count) = members.get_mut(&self.member) {
                *count -= 1;
                if *count == 0 {
                    members.remove(&self.member);
                }
            }
            if members.is_empty() {
                rooms.remove(&self.room);
            }
        }
    }
}
impl RoomHub {
    pub fn online_members(&self, room: &str) -> Vec<String> {
        let mut ids: Vec<_> = self
            .members
            .lock()
            .unwrap()
            .get(room)
            .map(|members| members.keys().cloned().collect())
            .unwrap_or_default();
        ids.sort();
        ids
    }
    pub fn member_joined(&self, room: String, member: String) -> MemberPresence {
        *self
            .members
            .lock()
            .unwrap()
            .entry(room.clone())
            .or_default()
            .entry(member.clone())
            .or_default() += 1;
        MemberPresence {
            members: self.members.clone(),
            room,
            member,
        }
    }
}
impl chat_application::room_directory::OnlineRoomCounts for RoomHub {
    fn snapshot(&self) -> HashMap<String, usize> {
        self.members
            .lock()
            .unwrap()
            .iter()
            .map(|(room, members)| (room.clone(), members.len()))
            .collect()
    }
}
#[cfg(test)]
mod online_member_tests {
    use super::*;
    use chat_application::room_directory::OnlineRoomCounts;
    #[test]
    fn tabs_are_deduplicated_and_last_disconnect_removes_member() {
        let hub = RoomHub::new(8);
        let a = hub.member_joined("r".into(), "a".into());
        let a2 = hub.member_joined("r".into(), "a".into());
        let b = hub.member_joined("r".into(), "b".into());
        assert_eq!(hub.snapshot()["r"], 2);
        drop(a);
        assert_eq!(hub.snapshot()["r"], 2);
        drop(a2);
        assert_eq!(hub.snapshot()["r"], 1);
        drop(b);
        assert!(hub.snapshot().is_empty());
    }
}

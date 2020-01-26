use matrix_nio::events::collections::all::{RoomEvent, StateEvent};
use matrix_nio::events::room::member::{MemberEvent, MembershipState};
use matrix_nio::events::room::message::{
    MessageEvent, MessageEventContent, TextMessageEventContent,
};
use matrix_nio::Room;
use url::Url;

use crate::server::Connection;
use crate::{Config, PLUGIN_NAME};
use std::cell::RefCell;
use std::rc::Rc;
use weechat::{Buffer, BufferSettings, Weechat};

pub(crate) struct RoomMember {
    nick: String,
    user_id: String,
    prefix: String,
    color: String,
}

pub(crate) struct RoomBuffer {
    server_name: String,
    homeserver: Url,
    room_id: String,
    prev_batch: Option<String>,
    typing_notice_time: Option<u64>,
    room: Room,
    printed_before_ack_queue: Vec<String>,
}

impl RoomBuffer {
    pub fn new(
        server_name: &str,
        connected_state: &Rc<RefCell<Option<Connection>>>,
        homeserver: &Url,
        config: &Config,
        room_id: String,
        own_user_id: &str,
    ) -> Self {
        let weechat = unsafe { Weechat::weechat() };

        let state = Rc::downgrade(connected_state);

        let buffer_settings = BufferSettings::new(&room_id.to_string())
            .input_data((state, room_id.to_owned()))
            .input_callback(async move |data, input| {
                {
                    let (client_rc, room_id) = data.unwrap();
                    let client_rc = client_rc.upgrade().expect(
                        "Can't upgrade server, server has been deleted?",
                    );
                    let client = client_rc.borrow();

                    if client.is_none() {
                        // buffer.print("Error not connected");
                        return;
                    }
                    if let Some(s) = client.as_ref() {
                        // TODO check for errors and print them out.
                        s.send_message(&room_id, &input).await;
                    }
                }
            })
            .close_callback(|weechat, buffer| {
                // TODO remove the roombuffer from the server here.
                Ok(())
            });

        let buffer = weechat
            .buffer_new(buffer_settings)
            .expect("Can't create new room buffer");

        RoomBuffer {
            server_name: server_name.to_owned(),
            homeserver: homeserver.clone(),
            room_id: room_id.clone(),
            prev_batch: None,
            typing_notice_time: None,
            room: Room::new(&room_id, &own_user_id.to_string()),
            printed_before_ack_queue: Vec::new(),
        }
    }

    pub fn get_weechat_buffer(&self) -> Option<Buffer> {
        let weechat = unsafe { Weechat::weechat() };
        weechat.buffer_search(PLUGIN_NAME, &self.room_id)
    }

    pub fn handle_membership_state(&mut self, event: MembershipState) {}

    pub fn handle_membership_event(&mut self, event: &MemberEvent) {
        let buffer = self.get_weechat_buffer().unwrap();
        let content = &event.content;

        let message = match content.membership {
            MembershipState::Join => "joined",
            MembershipState::Leave => "left",
            _ => return,
        };

        let message = format!(
            "{} ({}) has {} the room",
            content.displayname.as_ref().unwrap_or(&"".to_string()),
            event.state_key,
            message
        );
        let timestamp: u64 = event.origin_server_ts.into();
        let timestamp = timestamp / 1000;

        buffer.print_date_tags(timestamp as i64, &[], &message);
        self.room.handle_membership(&event);
    }

    pub fn handle_state_event(&mut self, event: StateEvent) {
        self.room.receive_state_event(&event);
    }

    pub fn handle_text_message(
        &mut self,
        sender: &str,
        timestamp: u64,
        content: &TextMessageEventContent,
    ) {
        let buffer = self.get_weechat_buffer().unwrap();
        let timestamp = timestamp / 1000;
        let message = format!("{}\t{}", sender, content.body);
        buffer.print_date_tags(timestamp as i64, &[], &message);
    }

    pub fn handle_room_message(&mut self, event: &MessageEvent) {
        let sender = &event.sender;
        let timestamp: u64 = event.origin_server_ts.into();

        match &event.content {
            MessageEventContent::Text(t) => {
                self.handle_text_message(&sender.to_string(), timestamp, t)
            }
            _ => (),
        }
    }

    pub fn handle_room_event(&mut self, event: RoomEvent) {
        match &event {
            RoomEvent::RoomMember(e) => self.handle_membership_event(e),
            RoomEvent::RoomMessage(m) => self.handle_room_message(m),
            event => {
                self.room.receive_timeline_event(event);
            }
        }
    }
}

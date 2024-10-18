use std::{net::TcpStream, str::from_utf8, sync::{Arc, Mutex}, thread::scope};

use serde::{Deserialize, Serialize};
use serde_with::rust::double_option;
use tungstenite::{WebSocket, stream::MaybeTlsStream};
use webrtc::{
    data_channel::RTCDataChannel,
    ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
    peer_connection::{
        RTCPeerConnection,
        sdp::session_description::RTCSessionDescription,
    },
};


pub struct Chat<'a>(Arc<Mutex<ChatImpl<'a>>>);

struct ChatImpl<'a> {
    ws: &'a mut WebSocket<MaybeTlsStream<TcpStream>>,
    rtc: &'a RTCPeerConnection,
    username: Option<String>,
    chat: Option<Arc<RTCDataChannel>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Msg {
    username: String,
    offer: Option<RTCSessionDescription>,
    answer: Option<RTCSessionDescription>,
    #[serde(default, skip_serializing_if="Option::is_none", with="double_option")]
    candidate: Option<Option<RTCIceCandidateInit>>,
}

impl<'a> Chat<'a> {
    pub async fn new(
        rtc: &'a RTCPeerConnection,
        ws: &'a mut WebSocket<MaybeTlsStream<TcpStream>>,
        username: Option<String>,
    ) -> Self {
        let chat = Arc::new(Mutex::new(ChatImpl::new(rtc, ws, username).await));

        scope(|s| {
            s.spawn(|| loop {
                let msg = chat.lock().unwrap().ws.read().unwrap().into_text().unwrap();
                let msg: Msg = serde_json::from_str(&msg).unwrap();
                chat.lock().unwrap().ws_on_message(msg);
            });
        });

        {
            let mut chat = chat.lock().unwrap();
            if let Some(username) = chat.username.clone() {
                chat.connect(&username).await;
            }
        }

        Self(chat)
    }

    pub fn send(&self, msg: String) {
        self.0.lock().unwrap().send(msg)
    }

    async fn init(&mut self) {
        let chat_impl = self.0.clone();
        self.0.lock().unwrap().rtc.on_peer_connection_state_change(Box::new(move |_| {
            chat_impl.lock().unwrap().rtc_on_state_change();
            Box::pin(async {})
        }));
        let chat_impl = self.0.clone();
        self.0.lock().unwrap().rtc.on_ice_connection_state_change(Box::new(move |_| {
            chat_impl.lock().unwrap().rtc_on_state_change();
            Box::pin(async {})
        }));
        let chat_impl = self.0.clone();
        self.0.lock().unwrap().rtc.on_ice_gathering_state_change(Box::new(move |_| {
            chat_impl.lock().unwrap().rtc_on_state_change();
            Box::pin(async {})
        }));
        let chat_impl = self.0.clone();
        self.0.lock().unwrap().rtc.on_signaling_state_change(Box::new(move |_| {
            chat_impl.lock().unwrap().rtc_on_state_change();
            Box::pin(async {})
        }));

        /*
        self.rtc.on_ice_candidate(Box::new(|c| {
            Box::pin(self.rtc_on_candidate(c))
        }));
        self.rtc.on_data_channel(Box::new(|c| {
            Box::pin(self.rtc_on_data_channel(c))
        }));
        */
    }
}

impl<'a> ChatImpl<'a> {
    async fn new(
        rtc: &'a RTCPeerConnection,
        ws: &'a mut WebSocket<MaybeTlsStream<TcpStream>>,
        username: Option<String>,
    ) -> Self {
        Self { ws, rtc, username, chat: None }
    }

    fn send(&self, msg: String) {
        println!("< {}", msg);
        if let Some(chat) = &self.chat {
            chat.send_text(msg);
        }
    }

    fn rtc_on_state_change(&self) {
        eprintln!("-- RTC state: peer: {}, ice: {}, gathering: {}, signal: {}",
            self.rtc.connection_state(),
            self.rtc.ice_connection_state(),
            self.rtc.ice_gathering_state(),
            self.rtc.signaling_state(),
        );
    }

    async fn connect(&mut self, username: &str) {
        self.init_chat(None).await;

        let offer = self.rtc.create_offer(None).await.unwrap();
        self.rtc.set_local_description(offer.clone()).await.unwrap();

        eprintln!("<- WS offer: {} {:?}", username, offer);
        let msg = Msg {
            username: username.to_owned(),
            offer: Some(offer),
            answer: None,
            candidate: None,
        };
        self.ws_send(&msg);
    }

    async fn ws_on_message(&mut self, msg: Msg) {
        eprintln!("-> WS message: {:?}", msg);
        if let Some(candidate) = msg.candidate {
            self.ws_on_candidate(msg.username, candidate).await;
        } else if let Some(answer) = msg.answer {
            self.ws_on_answer(msg.username, answer).await;
        } else if let Some(offer) = msg.offer {
            self.ws_on_offer(msg.username, offer).await;
        } else {
            self.ws_on_login(msg.username);
        }
    }

    fn ws_on_login(&mut self, username: String) {
        eprintln!("** Username: {}", username);
    }

    async fn ws_on_offer(
        &mut self,
        username: String,
        offer: RTCSessionDescription,
    ) {
        eprintln!("-> WS offer: {} {:?}", username, offer);
        self.rtc.set_remote_description(offer).await.unwrap();
        self.username = Some(username.clone());

        let answer = self.rtc.create_answer(None).await.unwrap();
        self.rtc.set_local_description(answer.clone()).await.unwrap();

        eprintln!("<- WS answer: {} {:?}", username, answer);
        let msg = Msg {
            username,
            answer: Some(answer),
            offer: None,
            candidate: None,
        };
        self.ws_send(&msg);
    }

    async fn ws_on_answer(
        &mut self,
        username: String,
        answer: RTCSessionDescription,
    ) {
        eprintln!("-> WS answer: {} {:?}", username, answer);
        self.rtc.set_remote_description(answer).await.unwrap();
    }

    async fn rtc_on_candidate(&mut self, candidate: Option<RTCIceCandidate>) {
        eprintln!("-- RTC candidate: {:?}", candidate);
        let candidate = candidate.map(|c| c.to_json().unwrap());

        eprintln!("<- WS candidate: {:?} {:?}", self.username, candidate);
        let msg = Msg {
            username: self.username.clone().unwrap(),
            candidate: Some(candidate),
            offer: None,
            answer: None,
        };
        self.ws_send(&msg);
    }

    async fn ws_on_candidate(
        &mut self,
        username: String,
        candidate: Option<RTCIceCandidateInit>,
    ) {
        eprintln!("-> WS candidate: {} {:?}", username, candidate);
        if let Some(candidate) = candidate {
            self.rtc.add_ice_candidate(candidate).await.unwrap();
        }
    }

    async fn rtc_on_data_channel(&mut self, chat: Arc<RTCDataChannel>) {
        self.init_chat(Some(chat)).await;
    }

    fn ws_send(&mut self, msg: &Msg) {
        eprintln!("<- Send to WS: {:?}", msg);
        let msg = serde_json::to_string(msg).unwrap();
        self.ws.send(msg.into()).unwrap();
    }

    async fn init_chat(&mut self, mut chat: Option<Arc<RTCDataChannel>>) {
        if chat.is_none() {
            chat = Some(self.rtc.create_data_channel("chat", None).await.unwrap());
        }
        let chat = chat.unwrap();

        chat.on_message(Box::new(|m| {
            Box::pin(async move {
                let s = from_utf8(&m.data).unwrap();
                println!("> {}", s);
            })
        }));

        self.chat = Some(chat);
    }
}

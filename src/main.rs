use std::{env::args, io::{stdin, BufRead}, str::from_utf8, sync::Arc};

use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_with::rust::double_option;
use tokio::{spawn, net::TcpStream, sync::{mpsc::channel, Mutex}};
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream, tungstenite::Message};
use webrtc::{
    api::APIBuilder,
    data_channel::RTCDataChannel,
    ice_transport::{
        ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
        ice_server::RTCIceServer,
    },
    peer_connection::{
        RTCPeerConnection,
        configuration::RTCConfiguration,
        sdp::session_description::RTCSessionDescription,
    },
};


#[derive(Debug, Deserialize, Serialize)]
struct Msg {
    username: String,
    offer: Option<RTCSessionDescription>,
    answer: Option<RTCSessionDescription>,
    #[serde(default, skip_serializing_if="Option::is_none", with="double_option")]
    candidate: Option<Option<RTCIceCandidateInit>>,
}

type WebSocket = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

static mut USERNAME: Option<String> = None;

#[tokio::main]
async fn main() {
    let rtc_cfg = RTCConfiguration {
        ice_servers: vec![
            RTCIceServer {
                urls: vec![String::from("stun:stun.1.google.com:19302")],
                ..RTCIceServer::default()
            },
        ],
        ..RTCConfiguration::default()
    };
    let rtc = APIBuilder::new().build().new_peer_connection(rtc_cfg).await.unwrap();
    let ws = connect_async("ws://localhost:9090").await.unwrap().0;
    let (ws_write, mut ws_read) = ws.split();
    let username = args().nth(1);

    let rtc = Arc::new(rtc);
    let ws_write = Arc::new(Mutex::new(ws_write));
    rtc_init(rtc.clone(), ws_write.clone());

    {
        let rtc = rtc.clone();
        let ws_write = ws_write.clone();
        spawn(async move {
            while let Some(msg) = ws_read.next().await {
                let msg = msg.unwrap().into_text().unwrap();
                let msg: Msg = serde_json::from_str(&msg).unwrap();
                let mut ws_write = ws_write.lock().await;
                ws_on_message(&rtc, &mut ws_write, msg).await;
            }
        });
    }

    let chat = {
        if let Some(username) = username {
            let mut ws_write = ws_write.lock().await;
            connect(&rtc, &mut ws_write, username).await
        } else {
            let (tx, mut rx) = channel(1);
            rtc.on_data_channel(Box::new(move |c| {
                let tx = tx.clone();
                Box::pin(async move {
                    chat_init(&c).await;
                    tx.send(c).await.unwrap();
                })
            }));
            rx.recv().await.unwrap()
        }
    };

    for msg in stdin().lock().lines() {
        let msg = msg.unwrap();
        chat_send(&chat, msg).await;
    }
}

fn rtc_init(rtc: Arc<RTCPeerConnection>, ws: Arc<Mutex<WebSocket>>) {
    let rtcc = rtc.clone();
    rtc.on_peer_connection_state_change(Box::new(move |_| {
        rtc_on_state_change(&rtcc);
        Box::pin(async {})
    }));
    let rtcc = rtc.clone();
    rtc.on_ice_connection_state_change(Box::new(move |_| {
        rtc_on_state_change(&rtcc);
        Box::pin(async {})
    }));
    let rtcc = rtc.clone();
    rtc.on_ice_gathering_state_change(Box::new(move |_| {
        rtc_on_state_change(&rtcc);
        Box::pin(async {})
    }));
    let rtcc = rtc.clone();
    rtc.on_signaling_state_change(Box::new(move |_| {
        rtc_on_state_change(&rtcc);
        Box::pin(async {})
    }));

    rtc.on_ice_candidate(Box::new(move |c| {
        let ws = ws.clone();
        Box::pin(async move {
            if let Some(username) = unsafe { USERNAME.clone() } {
                let mut ws = ws.lock().await;
                rtc_on_candidate(&mut ws, username, c).await;
            }
        })
    }));
}

fn rtc_on_state_change(rtc: &RTCPeerConnection) {
    eprintln!("-- RTC state: peer: {}, ice: {}, gathering: {}, signal: {}",
        rtc.connection_state(),
        rtc.ice_connection_state(),
        rtc.ice_gathering_state(),
        rtc.signaling_state(),
    );
}

async fn rtc_on_candidate(
    ws: &mut WebSocket,
    username: String,
    candidate: Option<RTCIceCandidate>,
) {
    eprintln!("-- RTC candidate: {:?}", candidate);
    let candidate = candidate.map(|c| c.to_json().unwrap());

    eprintln!("<- WS candidate: {:?} {:?}", username, candidate);
    let msg = Msg {
        username,
        candidate: Some(candidate),
        offer: None,
        answer: None,
    };
    ws_send(ws, &msg).await;
}

async fn ws_on_message(
    rtc: &RTCPeerConnection,
    ws: &mut WebSocket,
    msg: Msg,
) {
    //eprintln!("-> WS message: {:?}", msg);
    if let Some(candidate) = msg.candidate {
        ws_on_candidate(rtc, msg.username, candidate).await;
    } else if let Some(answer) = msg.answer {
        ws_on_answer(rtc, msg.username, answer).await;
    } else if let Some(offer) = msg.offer {
        ws_on_offer(rtc, ws, msg.username, offer).await;
    } else {
        ws_on_login(msg.username);
    }
}

fn ws_on_login(username: String) {
    eprintln!("** Username: {}", username);
}

async fn ws_on_offer(
    rtc: &RTCPeerConnection,
    ws: &mut WebSocket,
    username: String,
    offer: RTCSessionDescription,
) {
    eprintln!("-> WS offer: {} {:?}", username, offer);
    rtc.set_remote_description(offer).await.unwrap();
    unsafe {
        USERNAME = Some(username.clone());
    }

    let answer = rtc.create_answer(None).await.unwrap();
    rtc.set_local_description(answer.clone()).await.unwrap();

    eprintln!("<- WS answer: {} {:?}", username, answer);
    let msg = Msg {
        username,
        answer: Some(answer),
        offer: None,
        candidate: None,
    };
    ws_send(ws, &msg).await;
}

async fn ws_on_answer(
    rtc: &RTCPeerConnection,
    username: String,
    answer: RTCSessionDescription,
) {
    eprintln!("-> WS answer: {} {:?}", username, answer);
    rtc.set_remote_description(answer).await.unwrap();
}

async fn ws_on_candidate(
    rtc: &RTCPeerConnection,
    username: String,
    candidate: Option<RTCIceCandidateInit>,
) {
    eprintln!("-> WS candidate: {} {:?}", username, candidate);
    if let Some(candidate) = candidate {
        rtc.add_ice_candidate(candidate).await.unwrap();
    }
}

async fn ws_send(ws: &mut WebSocket, msg: &Msg) {
    //eprintln!("<- Send to WS: {:?}", msg);
    let msg = serde_json::to_string(msg).unwrap();
    ws.send(msg.into()).await.unwrap();
}

async fn connect(
    rtc: &RTCPeerConnection,
    ws: &mut WebSocket,
    username: String,
) -> Arc<RTCDataChannel> {
    let chat = rtc.create_data_channel("chat", None).await.unwrap();
    chat_init(&chat).await;

    let offer = rtc.create_offer(None).await.unwrap();
    rtc.set_local_description(offer.clone()).await.unwrap();

    eprintln!("<- WS offer: {} {:?}", username, offer);
    let msg = Msg {
        username,
        offer: Some(offer),
        answer: None,
        candidate: None,
    };
    ws_send(ws, &msg).await;

    chat
}

async fn chat_init(chat: &RTCDataChannel) {
    chat.on_open(Box::new(|| {
        eprintln!("-- Chat open");
        Box::pin(async {})
    }));
    chat.on_message(Box::new(|m| {
        Box::pin(async move {
            let s = from_utf8(&m.data).unwrap();
            println!("> {}", s);
        })
    }));
}

async fn chat_send(chat: &RTCDataChannel, msg: String) {
    println!("< {}", msg);
    chat.send_text(msg).await.unwrap();
}

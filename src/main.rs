use std::{env::args, io::{stdin, BufRead}, str::from_utf8, sync::Arc};

use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{spawn, net::TcpStream, sync::{mpsc::channel, Mutex}};
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream, tungstenite::Message};
use webrtc::{
    api::APIBuilder,
    data_channel::RTCDataChannel,
    ice_transport::{
        ice_credential_type::RTCIceCredentialType,
        ice_gatherer_state::RTCIceGathererState,
        ice_server::RTCIceServer,
    },
    peer_connection::{
        RTCPeerConnection,
        configuration::RTCConfiguration,
        sdp::sdp_type::RTCSdpType,
        sdp::session_description::RTCSessionDescription,
    },
};


#[derive(Debug, Deserialize, Serialize)]
struct Msg {
    username: String,
    desc: Option<RTCSessionDescription>,
}

type WebSocket = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

static mut USERNAME: Option<String> = None;

#[tokio::main]
async fn main() {
    let rtc_cfg = RTCConfiguration {
        ice_servers: vec![
            RTCIceServer {
                urls: vec![
                    //String::from("stun:stun.1.google.com:19302"),
                    String::from("stun:3.66.118.100:3478"),
                ],
                username: String::from("golem"),
                credential: String::from("melog"),
                credential_type: RTCIceCredentialType::Password,
            },
        ],
        ..RTCConfiguration::default()
    };
    let rtc = APIBuilder::new().build().new_peer_connection(rtc_cfg).await.unwrap();
    let ws = connect_async("ws://35.158.196.200:3049").await.unwrap().0;
    let (ws_write, mut ws_read) = ws.split();
    let username = args().nth(1);
    unsafe {
        USERNAME = username.clone();
    }

    let rtc = Arc::new(rtc);
    let ws_write = Arc::new(Mutex::new(ws_write));
    rtc_init(rtc.clone(), ws_write.clone());

    {
        let rtc = rtc.clone();
        spawn(async move {
            while let Some(msg) = ws_read.next().await {
                let msg = msg.unwrap().into_text().unwrap();
                let msg: Msg = serde_json::from_str(&msg).unwrap();
                ws_on_message(&rtc, msg).await;
            }
        });
    }

    let chat = {
        if username.is_some() {
            connect(&rtc).await
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
    rtc.on_ice_gathering_state_change(Box::new(move |state| {
        rtc_on_state_change(&rtcc);
        let ws = ws.clone();
        let rtcc = rtcc.clone();
        Box::pin(async move {
            if state == RTCIceGathererState::Complete {
                let mut ws = ws.lock().await;
                let msg = Msg {
                    username: unsafe { USERNAME.clone().unwrap() },
                    desc: rtcc.local_description().await,
                };
                ws_send(&mut ws, &msg).await;
            }
        })
    }));
    let rtcc = rtc.clone();
    rtc.on_signaling_state_change(Box::new(move |_| {
        rtc_on_state_change(&rtcc);
        Box::pin(async {})
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

async fn ws_on_message(
    rtc: &RTCPeerConnection,
    msg: Msg,
) {
    //eprintln!("-> WS message: {:?}", msg);
    if let Some(desc) = msg.desc {
        if desc.sdp_type == RTCSdpType::Answer {
            ws_on_answer(rtc, msg.username, desc).await;
        } else if desc.sdp_type == RTCSdpType::Offer {
            ws_on_offer(rtc, msg.username, desc).await;
        }
    } else {
        ws_on_login(msg.username);
    }
}

fn ws_on_login(username: String) {
    eprintln!("** Username: {}", username);
}

async fn ws_on_offer(
    rtc: &RTCPeerConnection,
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
}

async fn ws_on_answer(
    rtc: &RTCPeerConnection,
    username: String,
    answer: RTCSessionDescription,
) {
    eprintln!("-> WS answer: {} {:?}", username, answer);
    rtc.set_remote_description(answer).await.unwrap();
}

async fn ws_send(ws: &mut WebSocket, msg: &Msg) {
    eprintln!("<- Send to WS: {:?}", msg);
    let msg = serde_json::to_string(msg).unwrap();
    ws.send(msg.into()).await.unwrap();
}

async fn connect(
    rtc: &RTCPeerConnection,
) -> Arc<RTCDataChannel> {
    let chat = rtc.create_data_channel("chat", None).await.unwrap();
    chat_init(&chat).await;

    let offer = rtc.create_offer(None).await.unwrap();
    rtc.set_local_description(offer.clone()).await.unwrap();

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

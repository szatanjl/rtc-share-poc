mod chat;

use std::env::args;
use tungstenite::connect;
use webrtc::{
    api::APIBuilder,
    ice_transport::ice_server::RTCIceServer,
    peer_connection::configuration::RTCConfiguration,
};

#[tokio::main]
async fn main() {
    let username = args().nth(1);

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

    let mut ws = connect("ws://localhost:9090").unwrap().0;

    let _chat = chat::Chat::new(&rtc, &mut ws, username);
}

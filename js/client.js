var cfg = {
	iceServers: [
		{
			urls: [
				//"stun:stun.1.google.com:19302",
				"stun:3.66.118.100:3478",
			],
			username: "golem",
			credential: "melog",
			credentialType: "password",
		},
	],
};
var rtc = new RTCPeerConnection(cfg);
var chat = null;
var server = new WebSocket("ws://35.158.196.200:3049");

var username = document.getElementById("username");
var connect = document.getElementById("connect");
var message = document.getElementById("message");
var send = document.getElementById("send");


server.onopen = function() {
	console.info("Server connected");
};

server.onerror = function(err) {
	console.error("Server error:", err);
};

server.onclose = function() {
	console.error("Server closed");
};

rtc.onconnectionstatechange =
rtc.oniceconnectionstatechange =
rtc.onicegatheringstatechange =
rtc.onsignalingstatechange =
function() {
	console.debug("Rtc state:", {
		connectionState: rtc.connectionState,
		iceConnectionState: rtc.iceConnectionState,
		iceGatheringState: rtc.iceGatheringState,
		signalingState: rtc.signalingState,
	});
}


server.onmessage = function(msg) {
	var data = JSON.parse(msg.data);
	console.debug("Server message:", data);
	if (data.username == null) {
		console.error("Server error: no username in message");
		return;
	}

	if (data.answer != null) {
		serverOnAnswer(data.username, data.answer);
	} else if (data.offer != null) {
		serverOnOffer(data.username, data.offer);
	} else {
		serverOnLogin(data.username);
	}
};

function serverOnLogin(username) {
	console.info("Username:", username);
}

connect.onclick = function() {
	chat = initChat();
	rtc.createOffer().then(function(offer) {
		console.debug("Before Connect(offer):", username.value, offer);
		setLocalDescription(rtc, offer).then(() => {
			offer = rtc.localDescription;
			console.info("Connect(offer):", username.value, offer);
			serverSend({ offer });
		});
	});
};

function serverOnOffer(user, offer) {
	console.info("Server offer:", user, offer);
	rtc.setRemoteDescription(new RTCSessionDescription(offer));
	rtc.createAnswer().then(function(answer) {
		console.debug("Before Connect(anwer):", user, answer);
		username.value = user;
		setLocalDescription(rtc, answer).then(() => {
			answer = rtc.localDescription;
			console.info("Connect(anwer):", user, answer);
			serverSend({ answer });
		});
	});
}

function setLocalDescription(rtc, desc) {
	rtc.setLocalDescription(desc);
	return new Promise((resolve, reject) => {
		rtc.onicecandidate = function(ev) {
			console.debug("Rtc new ICE candidate:", ev.candidate);
			if (ev.candidate == null) {
				console.info("Rtc ICE candidates gathered");
				resolve();
			}
		}
		rtc.onicecandidateerror = function(e) {
			console.debug("Rtc new ICE candidate error:", e.message);
			reject();
		}
	});
}

function serverOnAnswer(username, answer) {
	console.info("Server answer:", username, answer);
	rtc.setRemoteDescription(new RTCSessionDescription(answer));
}

function serverSend(msg) {
	msg.username = username.value;
	console.debug("Send to server:", msg);
	server.send(JSON.stringify(msg));
}

rtc.ondatachannel = function(ev) {
	chat = initChat(ev.channel);
}

function initChat(chat) {
	if (chat == null) {
		chat = rtc.createDataChannel("chat");
	}
	console.info("Init data channel:", chat);

	chat.onopen = function() {
		console.info("Chat open");
	}

	chat.onerror = function(err) {
		console.error("Chat error:", err);
	}

	chat.onclose = function() {
		console.error("Chat close");
	}

	chat.onmessage = function(ev) {
		console.info("Chat message:", ev.data);
	}

	return chat;
}

send.onclick = function() {
	var msg = message.value;
	console.debug("Send to chat:", msg);
	chat.send(msg);
}

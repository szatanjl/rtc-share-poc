var { argv } = require("node:process");
var readline = require("readline");
var { IceCandidate, RTCPeerConnection, RTCSessionDescription } = require("werift");
var WebSocket = require("ws");


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
var username = null;


server.on("open", function() {
	console.info("Server connected");
});

server.on("error", function(err) {
	console.error("Server error:", err);
});

server.on("close", function() {
	console.error("Server closed");
});

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


server.on("message", function(msg) {
	var data = JSON.parse(msg);
	console.debug("Server message:", data);
	if (data.username == null) {
		console.error("Server error: no username in message");
		return;
	}

	if (data.candidate !== undefined) {
		serverOnCandidate(data.username, data.candidate);
	} else if (data.answer != null) {
		serverOnAnswer(data.username, data.answer);
	} else if (data.offer != null) {
		serverOnOffer(data.username, data.offer);
	} else {
		serverOnLogin(data.username);
	}
});

function serverOnLogin(username) {
	console.info("Username:", username);
}

function connect() {
	chat = initChat();
	rtc.createOffer().then(function(offer) {
		console.info("Connect(offer):", username, offer);
		rtc.setLocalDescription(offer);
		serverSend({ offer });
	});
}

function serverOnOffer(user, offer) {
	console.info("Server offer:", user, offer);
	rtc.setRemoteDescription(new RTCSessionDescription(offer.sdp, offer.type));
	rtc.createAnswer().then(function(answer) {
		console.info("Connect(anwer):", user, answer);
		rtc.setLocalDescription(answer);
		username = user;
		serverSend({ answer });
	});
}

function serverOnAnswer(username, answer) {
	console.info("Server answer:", username, answer);
	rtc.setRemoteDescription(new RTCSessionDescription(answer.sdp, answer.type));
}

rtc.onicecandidate = function(ev) {
	console.info("Rtc new ICE candidate:", ev.candidate);
	if (ev.candidate == null) {
		ev.candidate = null;
	}
	serverSend({ candidate: ev.candidate });
};

function serverOnCandidate(username, candidate) {
	if (candidate != null) {
		var candidate = new IceCandidate(candidate);
	}
	console.info("Server new ICE candidate:", username, candidate);
	rtc.addIceCandidate(candidate);
}

function serverSend(msg) {
	msg.username = username;
	console.debug("Send to server:", msg);
	server.send(JSON.stringify(msg));
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

username = argv[2];
if (username != null) {
	connect();
} else {
	rtc.ondatachannel = function(ev) {
		chat = initChat(ev.channel);
	}
}


const rl = readline.createInterface({
	input: process.stdin,
	output: process.stdout,
	terminal: true,
});

rl.on("line", msg => chat.send(msg));

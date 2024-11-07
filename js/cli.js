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


server.on("error", function(err) {
	console.error("Server error:", err);
});

server.on("close", function() {
	console.error("Server closed");
});

rtc.onconnectionstatechange =
rtc.onsignalingstatechange =
function() {
	console.debug("Rtc state:", {
		connectionState: rtc.connectionState,
		iceConnectionState: rtc.iceConnectionState,
		iceGatheringState: rtc.iceGatheringState,
		signalingState: rtc.signalingState,
	});
};

rtc.iceGatheringStateChange.subscribe(() => {
	console.debug("Rtc state:", {
		connectionState: rtc.connectionState,
		iceConnectionState: rtc.iceConnectionState,
		iceGatheringState: rtc.iceGatheringState,
		signalingState: rtc.signalingState,
	});

	var state = rtc.iceGatheringState;
	var desc = rtc.localDescription;
	if (state === "complete") {
		console.info(`Connect(${desc.type}):`, username, desc);
		serverSend({ desc });
	}
});


server.on("message", function(msg) {
	var data = JSON.parse(msg);
	console.debug("Server message:", data);
	if (data.username == null) {
		console.error("Server error: no username in message");
		return;
	}

	if (data.desc != null && data.desc.type === "answer") {
		serverOnAnswer(data.username, data.desc);
	} else if (data.desc != null && data.desc.type === "offer") {
		serverOnOffer(data.username, data.desc);
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
		console.debug("Create offer:", username, offer);
		rtc.setLocalDescription(offer);
	});
}

function serverOnOffer(user, offer) {
	console.info("Server offer:", user, offer);
	rtc.setRemoteDescription(new RTCSessionDescription(offer.sdp, offer.type));
	rtc.createAnswer().then(function(answer) {
		console.debug("Create answer:", user, answer);
		username = user;
		rtc.setLocalDescription(answer);
	});
}

function serverOnAnswer(username, answer) {
	console.info("Server answer:", username, answer);
	rtc.setRemoteDescription(new RTCSessionDescription(answer.sdp, answer.type));
}

function serverSend(msg) {
	msg.username = username;
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


server.on("open", function() {
	console.info("Server connected");

	username = argv[2];
	if (username != null) {
		console.info("Connect to:", username);
		connect();
	}

	const rl = readline.createInterface({
		input: process.stdin,
		output: process.stdout,
		terminal: true,
	});
	rl.on("line", msg => chat.send(msg));
});

var uuid = require("uuid");
var ws = require("ws");

var server = new ws.WebSocketServer({ port: 9090 });
var users = {};

server.on("connection", function(connection) {
	var username = uuid.v4();
	users[username] = connection;
	console.info("User connect:", username);
	connection.send(JSON.stringify({ username }));

	connection.on("message", function(msg) {
		var data = JSON.parse(msg);
		console.debug("Recv:", data);
		if (data.username == null) {
			console.error("Error: username missing");
			return;
		}
		var conn = users[data.username];
		if (conn == null) {
			console.error("Error: username not found:", data.username);
			return;
		}
		data.username = username;
		console.debug("Send:", data);
		conn.send(JSON.stringify(data));
	});

	connection.on("error", function(err) {
		console.error("Error:", username, err);
	});

	connection.on("close", function() {
		console.info("User disconnect:", username);
		delete users[username];
	});
});

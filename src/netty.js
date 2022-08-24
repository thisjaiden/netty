var connection = null;
var connection_buffer = [];
var last_address = null;

export function start_ws(address) {
    last_address = address;
    console.log("Starting WS Connection...");
    try {
        connection = new WebSocket(address);
    }
    catch (e) {
        console.log("Caught an error: " + e);
        return 1;
    }
    connection.binaryType = "arraybuffer";
    connection.addEventListener("message", (event) => {
        if (event.data instanceof ArrayBuffer) {
            // binary frame
            connection_buffer.push(event.data);
        }
        else {
            // text frame
            console.log("WARN: Recieved text data when only binary data should be sent: " + event.data);
        }
    });
    connection.addEventListener("open", (event) => {
        console.log("WebSocket opened.");
    });
    connection.addEventListener("close", (event) => {
        console.log("WebSocket closed.");
    });
    connection.addEventListener("error", (event) => {
        console.log("WebSocket ERROR: ", event);
    });
    return 0;
}

export function get_ws() {
    if (connection_buffer.length > 0) {
        value = connection_buffer.shift();
        return value;
    }
    else {
        return new ArrayBuffer();
    }
}

export function send_ws(input_data) {
    if (connection.readyState == 1) {
        connection.send(input_data);
    }
    else if (connection.readyState == 3) {
        start_ws(last_address);
        console.log("WARN: Connection is not ready to send (" + connection.readyState + "), rebooting and delaying 2000ms");
        setTimeout(send_ws, 2000, input_data);
    }
    else {
        console.log("WARN: Connection is not ready to send (" + connection.readyState + "), delaying 2000ms");
        setTimeout(send_ws, 2000, input_data);
    }
}

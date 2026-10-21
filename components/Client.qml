import QtQuick
import Quickshell.Io

// The overlay's connection to thodud (SPEC section 7): one JSON request per
// line out and one reply per line back, in the order sent. Requests made
// before the connection is up wait for it.
Item {
    id: client
    visible: false

    property var waiting: []

    signal reply(var reply)
    // The connection closed: thodud stopped or restarted.
    signal lost

    function connectTo(path) {
        if (wire.path !== path) {
            if (wire.connected)
                wire.connected = false;
            wire.path = path;
        }
        if (!wire.connected)
            wire.connected = true;
    }

    function send(request) {
        const line = JSON.stringify(Object.assign({ v: 1 }, request)) + "\n";
        if (!wire.connected) {
            client.waiting = client.waiting.concat([line]);
            return;
        }
        wire.write(line);
        wire.flush();
    }

    Socket {
        id: wire

        parser: SplitParser {
            onRead: data => {
                let parsed = null;
                try {
                    parsed = JSON.parse(data);
                } catch (e) {
                    console.warn("thodu: a reply that is not JSON: " + String(data).slice(0, 200));
                    return;
                }
                client.reply(parsed);
            }
        }

        onConnectionStateChanged: {
            if (!connected) {
                client.waiting = [];
                client.lost();
                return;
            }
            const lines = client.waiting;
            client.waiting = [];
            for (const line of lines)
                wire.write(line);
            wire.flush();
        }

        // Quickshell 0.3.2 finishes its own error handling after this
        // signal; setting connected here crashes the shell. A connect that
        // fails leaves the requests waiting, and the session view's silence
        // timer ends the session.
        onError: error => console.warn("thodu: the socket to thodud failed: " + error)
    }
}

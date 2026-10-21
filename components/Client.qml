import QtQuick
import Quickshell.Io

// The overlay's connection to thodud for requests (SPEC section 7): one
// JSON request per line out and one reply per line back, in the order
// sent. Requests made before the connection is up wait for it.
Item {
    id: client
    visible: false

    property string path: ""
    property var wire: null
    property var waiting: []
    readonly property bool connected: wire !== null && wire.connected

    signal reply(var reply)
    // The connection closed: thodud stopped or restarted.
    signal lost

    // A fresh socket for each connect: Quickshell drops a socket whose
    // connect failed, as Nodi's SocketReader found.
    function connectTo(path) {
        if (client.connected && client.path === path)
            return;
        if (client.wire) {
            const spent = client.wire;
            client.wire = null;
            if (spent.connected)
                spent.connected = false;
            spent.destroy();
        }
        client.path = path;
        client.wire = wireType.createObject(client, { path: path });
        client.wire.connected = true;
    }

    function send(request) {
        const line = JSON.stringify(Object.assign({ v: 1 }, request)) + "\n";
        if (!client.connected) {
            client.waiting = client.waiting.concat([line]);
            return;
        }
        client.wire.write(line);
        client.wire.flush();
    }

    Component {
        id: wireType

        Socket {
            id: transport

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
                if (transport !== client.wire)
                    return;
                if (!connected) {
                    client.waiting = [];
                    client.lost();
                    return;
                }
                const lines = client.waiting;
                client.waiting = [];
                for (const line of lines)
                    transport.write(line);
                transport.flush();
            }

            // Quickshell 0.3.2 finishes its own error handling after this
            // signal; setting connected here crashes the shell. A connect
            // that fails leaves the requests waiting, and the session view's
            // silence timer ends the session.
            onError: error => console.warn("thodu: the socket to thodud failed: " + error)
        }
    }
}

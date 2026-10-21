pragma ComponentBehavior: Bound

import QtQuick
import Quickshell.Io

// The overlay's standing connection to thodud (SPEC section 7, listen):
// it says listen once, and thodud pushes each new session down it, which
// is much faster than a summon through omarchy-shell (REQ-PERF-01). While
// thodud is away it tries again every retryMs.
Item {
    id: listener
    visible: false

    property string path: ""
    property int retryMs: 1000
    property var wire: null
    readonly property bool listening: wire !== null && wire.connected

    signal opened(var event)

    function start(path) {
        if (path === listener.path && listener.listening)
            return;
        listener.path = path;
        listener.reconnect();
    }

    // A fresh socket each time: Quickshell drops a socket whose connect
    // failed, as Nodi's SocketReader found.
    function reconnect() {
        if (listener.wire) {
            const spent = listener.wire;
            listener.wire = null;
            if (spent.connected)
                spent.connected = false;
            spent.destroy();
        }
        if (!listener.path)
            return;
        listener.wire = wireType.createObject(listener, { path: listener.path });
        listener.wire.connected = true;
        retry.restart();
    }

    Timer {
        id: retry
        interval: listener.retryMs
        onTriggered: {
            if (!listener.listening)
                listener.reconnect();
        }
    }

    Component {
        id: wireType

        Socket {
            id: transport

            parser: SplitParser {
                onRead: data => {
                    let event = null;
                    try {
                        event = JSON.parse(data);
                    } catch (e) {
                        return;
                    }
                    if (event && event.event === "open")
                        listener.opened(event);
                }
            }

            onConnectionStateChanged: {
                if (transport !== listener.wire)
                    return;
                if (transport.connected) {
                    retry.stop();
                    transport.write(JSON.stringify({ v: 1, op: "listen" }) + "\n");
                    transport.flush();
                } else {
                    retry.restart();
                }
            }

            // Never set connected here: Quickshell 0.3.2 crashes. The retry
            // timer makes a new socket instead.
            onError: error => {}
        }
    }
}

import QtQuick
import Quickshell
import "components"
import "lib/BarScan.js" as BarScan

// Thodu's bar widget, which draws nothing: once a second it finds the
// bar's clickable items and, when they change, tells thodud, which offers
// them as shell targets in sessions on this monitor (REQ-SHELL-01).
Item {
    id: probe

    // Set by the bar host.
    property var bar: null
    property var shell: null
    property var manifest: null

    property string last: ""
    readonly property string socketPath: Quickshell.env("XDG_RUNTIME_DIR") + "/thodu.sock"

    visible: false
    implicitWidth: 0
    implicitHeight: 0

    function report() {
        const items = BarScan.scan(BarScan.rootOf(probe), probe);
        const request = { op: "shell-items", screen: probe.Screen.name, items: items };
        const line = JSON.stringify(request);
        if (line === probe.last && client.connected)
            return;
        probe.last = line;
        client.connectTo(probe.socketPath);
        client.send(request);
    }

    Client {
        id: client
        keepLatest: true
    }

    Timer {
        interval: 1000
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: probe.report()
    }
}

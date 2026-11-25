import QtQuick
import "../lib/BarScan.js" as BarScan

// REQ-SHELL-01: asks for a bar scan after each Hyprland event that may move
// the bar's items, so a click is checked against current items rather than
// ones up to a second old. It asks at once, and again once the bar has
// settled, because the bar may lay out its new items only after the event
// has arrived.
Item {
    id: rescan

    visible: false
    width: 0
    height: 0

    // What emits rawEvent(event) with an event name: Hyprland in the shell.
    property var events: null
    property int settleMs: 100

    signal due

    function handle(name) {
        if (!BarScan.rescansOn(name))
            return;
        Qt.callLater(rescan.emitDue);
        settle.restart();
    }

    function emitDue() {
        rescan.due();
    }

    Connections {
        target: rescan.events
        function onRawEvent(event) {
            rescan.handle(event.name);
        }
    }

    Timer {
        id: settle
        interval: rescan.settleMs
        onTriggered: rescan.emitDue()
    }
}

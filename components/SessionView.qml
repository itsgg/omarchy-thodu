pragma ComponentBehavior: Bound

import QtQuick

// One session drawn over a monitor: the labels thodud placed, the focused
// target and a status line. It turns keys into requests and replies into
// what it draws, and holds no target logic (SPEC section 4). Thodu.qml
// carries its requests over the socket; tests/qml/tst_session.qml uses a
// fake IPC. Plain QtQuick, so qmltestrunner can load it.
FocusScope {
    id: view

    property string session: ""
    // The targets with their labels, from the first full session reply.
    property var targets: []
    property bool loaded: false
    // The monitor's logical area and name; labels are drawn at their
    // logical place less the area's origin.
    property var area: ({ x: 0, y: 0, w: 0, h: 0 })
    property string monitorName: ""
    // Ids of the targets the search and the typed label still keep.
    property var kept: ({})
    property string focusedId: ""
    property string search: ""
    // With labels_only, / started a search (REQ-SEARCH-02).
    property bool searching: false
    property string labelPrefix: ""
    property string status: ""
    property bool active: false

    // A brief session request this often tells a live thodud from a dead
    // one, and shows changes made by keys sent over the IPC in test mode.
    property int heartbeatMs: 200
    // REQ-SESSION-03: this long without a reply ends the session, inside
    // the second the spec allows.
    property int silenceMs: 800

    property color background: "#1e1e2e"
    property color foreground: "#cdd6f4"
    property color border: "#cdd6f4"
    property color selectedBackground: "#313244"
    property color selectedText: "#89b4fa"
    property string fontFamily: "monospace"

    signal send(var request)
    // Why the session ended, and whether thodud now acts: the overlay must
    // then hide and say so with a hidden request.
    signal finished(string why, bool acting)

    // Start drawing session `id`; `reply` is its full session reply when
    // thodud pushed it, else the view asks for it.
    function begin(id, reply) {
        view.session = id;
        view.targets = [];
        view.loaded = false;
        view.area = { x: 0, y: 0, w: 0, h: 0 };
        view.monitorName = "";
        view.kept = {};
        view.focusedId = "";
        view.search = "";
        view.searching = false;
        view.labelPrefix = "";
        view.status = "";
        view.active = true;
        view.focus = true;
        silence.restart();
        if (reply)
            view.apply(reply);
        else
            view.send({ op: "session", session: id });
    }

    function end(why, acting) {
        if (!view.active)
            return;
        view.active = false;
        view.focus = false;
        silence.stop();
        view.finished(why, acting);
    }

    function apply(reply) {
        if (!view.active || !reply)
            return;
        if (reply.session !== undefined && reply.session !== view.session)
            return;
        silence.restart();
        if (!reply.ok) {
            view.end(reply.message || "thodud refused the session", false);
            return;
        }
        if (reply.targets && !view.loaded) {
            view.targets = reply.targets;
            if (reply.monitor) {
                view.area = reply.monitor.area;
                view.monitorName = reply.monitor.name;
            }
            view.loaded = true;
        }
        if (reply.status)
            view.status = reply.status;
        const shown = reply.view;
        if (!shown)
            return;
        const kept = {};
        for (const id of shown.visible)
            kept[id] = true;
        view.kept = kept;
        view.focusedId = shown.focused || "";
        view.search = shown.search || "";
        view.searching = shown.searching === true;
        view.labelPrefix = shown.label_prefix || "";
        if (shown.state === "acting")
            view.end("acting", true);
        else if (shown.state === "closed")
            view.end("closed", false);
    }

    // A key as SPEC section 7 names it, or null for keys a session ignores.
    function keyRequest(key, modifiers) {
        const shift = (modifiers & Qt.ShiftModifier) !== 0;
        const ctrl = (modifiers & Qt.ControlModifier) !== 0;
        let name = "";
        if (key === Qt.Key_Escape)
            name = "Escape";
        else if (key === Qt.Key_Return || key === Qt.Key_Enter)
            name = "Return";
        else if (key === Qt.Key_Tab)
            name = "Tab";
        else if (key === Qt.Key_Backtab)
            name = "Backtab";
        else if (key === Qt.Key_Backspace)
            name = "BackSpace";
        else if (key === Qt.Key_Space)
            name = "space";
        else if (key === Qt.Key_Slash)
            name = "/";
        else if (key >= Qt.Key_A && key <= Qt.Key_Z)
            name = String.fromCharCode(key).toLowerCase();
        else
            return null;
        return { op: "key", session: view.session, key: name, shift: shift, ctrl: ctrl };
    }

    function labelText(label) {
        const typed = view.labelPrefix.length;
        const head = label.slice(0, typed).toUpperCase();
        const rest = label.slice(typed).toUpperCase();
        if (!head)
            return rest;
        return "<font color=\"" + view.selectedText + "\">" + head + "</font>" + rest;
    }

    Keys.onPressed: event => {
        event.accepted = true;
        if (!view.active)
            return;
        const request = view.keyRequest(event.key, event.modifiers);
        if (!request)
            return;
        view.send(request);
        // REQ-SESSION-02: the keyboard goes back at once, whatever thodud
        // answers, so no key after Escape reaches the overlay.
        if (request.key === "Escape")
            view.end("escape", false);
    }

    Timer {
        id: heartbeat
        interval: view.heartbeatMs
        repeat: true
        running: view.active
        onTriggered: view.send({ op: "session", session: view.session, brief: true })
    }

    Timer {
        id: silence
        interval: view.silenceMs
        onTriggered: view.end("thodud stopped answering", false)
    }

    // The focused target's bounds, for a search that has not narrowed to
    // one label yet.
    Rectangle {
        id: focusRing
        readonly property var target: view.targets.find(t => t.id === view.focusedId) || null
        visible: view.loaded && target !== null
        x: target ? target.bounds.x - view.area.x - 2 : 0
        y: target ? target.bounds.y - view.area.y - 2 : 0
        width: target ? target.bounds.w + 4 : 0
        height: target ? target.bounds.h + 4 : 0
        color: "transparent"
        radius: 4
        border.width: 2
        border.color: view.selectedText
    }

    Repeater {
        id: labels
        model: view.loaded ? view.targets : []

        delegate: Rectangle {
            id: chip
            required property var modelData
            readonly property bool focusedOne: view.focusedId === modelData.id
            objectName: "label-" + modelData.label
            visible: view.kept[modelData.id] === true
            x: modelData.placed.x - view.area.x
            y: modelData.placed.y - view.area.y
            width: modelData.placed.w
            height: modelData.placed.h
            radius: 3
            color: focusedOne ? view.selectedBackground : view.background
            border.width: 1
            border.color: focusedOne ? view.selectedText : view.border

            Text {
                anchors.centerIn: parent
                text: view.labelText(chip.modelData.label)
                textFormat: Text.StyledText
                color: view.foreground
                font.family: view.fontFamily
                font.pixelSize: 12
                font.bold: true
            }
        }
    }

    Rectangle {
        id: statusLine
        visible: view.loaded && statusText.text.length > 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 24
        width: statusText.implicitWidth + 24
        height: statusText.implicitHeight + 12
        radius: 6
        color: view.background
        border.width: 1
        border.color: view.border

        Text {
            id: statusText
            anchors.centerIn: parent
            text: view.search || view.searching ? "/" + view.search : view.status
            color: view.foreground
            font.family: view.fontFamily
            font.pixelSize: 13
        }
    }
}

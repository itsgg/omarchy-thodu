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
    // The targets' version: the grid replaces them as cells are chosen.
    property int generation: 0
    property bool fetching: false
    // After a click, the keyboard is kept this long for a second Return,
    // with nothing drawn and the pointer passing through (REQ-ACT-02).
    property bool lingering: false
    property int lingerMs: 300

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
    // thodud clicks while the view keeps the keyboard: the overlay must let
    // the pointer through and say it is out of the way.
    signal lingered

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
        view.generation = 0;
        view.fetching = false;
        view.lingering = false;
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
        view.lingering = false;
        silence.stop();
        linger.stop();
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
        if (reply.workspace_changed === true) {
            view.end("workspace changed", false);
            return;
        }
        const shownGeneration = reply.view ? (reply.view.generation || 0) : view.generation;
        if (reply.targets && (!view.loaded || shownGeneration !== view.generation)) {
            view.targets = reply.targets;
            view.generation = shownGeneration;
            view.fetching = false;
            if (reply.monitor) {
                view.area = reply.monitor.area;
                view.monitorName = reply.monitor.name;
            }
            view.loaded = true;
        } else if (view.loaded && shownGeneration !== view.generation && !view.fetching) {
            // Keys sent over the IPC changed the targets; fetch them.
            view.fetching = true;
            view.send({ op: "session", session: view.session });
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
        if (shown.state === "acting" && shown.linger) {
            if (!view.lingering) {
                view.lingering = true;
                linger.restart();
                view.lingered();
            }
        } else if (shown.state === "acting") {
            view.end("acting", true);
        } else if (shown.state === "closed") {
            view.end("closed", false);
        }
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

    // The smallest rectangle holding every rectangle in `rects`.
    function bounding(rects) {
        if (rects.length === 0)
            return { x: 0, y: 0, w: 0, h: 0 };
        let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
        for (const r of rects) {
            x0 = Math.min(x0, r.x);
            y0 = Math.min(y0, r.y);
            x1 = Math.max(x1, r.x + r.w);
            y1 = Math.max(y1, r.y + r.h);
        }
        return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
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
        // After a click only a plain Return means anything: a double click.
        // Any other key ends the session at once, and is not seen again.
        else if (view.lingering && (request.key !== "Return" || request.shift || request.ctrl))
            view.end("a key in the wait after a click", false);
    }

    Timer {
        id: heartbeat
        interval: view.heartbeatMs
        repeat: true
        running: view.active
        onTriggered: view.send({ op: "session", session: view.session, brief: true })
    }

    Timer {
        id: linger
        interval: view.lingerMs
        onTriggered: view.end("no second Return", false)
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
        visible: view.loaded && target !== null && !view.lingering
        x: target ? target.bounds.x - view.area.x - 2 : 0
        y: target ? target.bounds.y - view.area.y - 2 : 0
        width: target ? target.bounds.w + 4 : 0
        height: target ? target.bounds.h + 4 : 0
        color: "transparent"
        radius: 4
        border.width: 2
        border.color: view.selectedText
    }

    // The grid's cells (REQ-GRID-01). Below the monitor's level they are
    // too small for their labels, so the area is outlined and its cells are
    // drawn again, magnified, in a panel beside it.
    readonly property var gridCells: view.loaded && !view.lingering ? view.targets.filter(t => t.source === "grid") : []
    readonly property var gridArea: view.bounding(view.gridCells.map(t => t.bounds))
    readonly property bool zoomed: view.gridCells.length > 0 && view.gridArea.w * view.gridArea.h < view.area.w * view.area.h

    Repeater {
        model: view.zoomed ? [] : view.gridCells

        delegate: Rectangle {
            required property var modelData
            x: modelData.bounds.x - view.area.x
            y: modelData.bounds.y - view.area.y
            width: modelData.bounds.w
            height: modelData.bounds.h
            color: "transparent"
            border.width: 1
            border.color: Qt.rgba(view.border.r, view.border.g, view.border.b, 0.35)
            visible: view.kept[modelData.id] === true
        }
    }

    Rectangle {
        id: zoomArea
        visible: view.zoomed
        x: view.gridArea.x - view.area.x - 2
        y: view.gridArea.y - view.area.y - 2
        width: view.gridArea.w + 4
        height: view.gridArea.h + 4
        color: "transparent"
        border.width: 2
        border.color: view.selectedText
    }

    Rectangle {
        id: zoom
        objectName: "zoom"
        readonly property real factor: view.zoomed ? Math.min(420 / view.gridArea.w, 300 / view.gridArea.h) : 1
        readonly property real below: view.gridArea.y - view.area.y + view.gridArea.h + 16
        visible: view.zoomed
        width: view.gridArea.w * factor
        height: view.gridArea.h * factor
        x: Math.max(8, Math.min(view.gridArea.x - view.area.x + view.gridArea.w / 2 - width / 2, view.area.w - width - 8))
        y: below + height + 8 <= view.area.h ? below : Math.max(8, view.gridArea.y - view.area.y - height - 16)
        color: view.background
        border.width: 1
        border.color: view.selectedText

        Repeater {
            model: view.zoomed ? view.gridCells : []

            delegate: Rectangle {
                id: zoomCell
                required property var modelData
                objectName: "zoom-" + modelData.label
                x: (modelData.bounds.x - view.gridArea.x) * zoom.factor
                y: (modelData.bounds.y - view.gridArea.y) * zoom.factor
                width: modelData.bounds.w * zoom.factor
                height: modelData.bounds.h * zoom.factor
                color: view.focusedId === modelData.id ? view.selectedBackground : "transparent"
                border.width: 1
                border.color: Qt.rgba(view.border.r, view.border.g, view.border.b, 0.35)
                visible: view.kept[modelData.id] === true

                Text {
                    anchors.centerIn: parent
                    text: view.labelText(zoomCell.modelData.label)
                    textFormat: Text.StyledText
                    color: view.foreground
                    font.family: view.fontFamily
                    font.pixelSize: 14
                    font.bold: true
                }
            }
        }
    }

    Repeater {
        id: labels
        model: view.loaded ? view.targets : []

        delegate: Rectangle {
            id: chip
            required property var modelData
            readonly property bool focusedOne: view.focusedId === modelData.id
            objectName: "label-" + modelData.label
            visible: view.kept[modelData.id] === true && !view.lingering && !(view.zoomed && modelData.source === "grid")
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
        visible: view.loaded && statusText.text.length > 0 && !view.lingering
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

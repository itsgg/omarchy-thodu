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
    // Workspace changes invalidate the visible targets and must release the
    // monitor-wide overlay before another workspace is shown (REQ-SESSION-01).
    function workspaceDeactivated() {
        if (view.active)
            view.end("workspace changed", false);
    }

    // Ids of the targets the search and the typed label still keep.
    property var kept: ({})
    property string focusedId: ""
    property bool labelSelected: false
    property string search: ""
    property bool labelsOnly: false
    property bool scrollMode: false
    // With labels_only, / started a search (REQ-SEARCH-02).
    property bool searching: false
    property string labelPrefix: ""
    property string status: ""
    property string statusDisplay: "auto"
    property bool statusPersistent: false
    property bool statusExpired: false
    property int statusTimeoutMs: 2000
    property bool active: false
    property bool collecting: false
    // The targets' version: the grid replaces them as cells are chosen.
    property int generation: 0
    property bool fetching: false
    // After a click, the keyboard is kept this long for a second Return,
    // with nothing drawn and the pointer passing through (REQ-ACT-02).
    property bool lingering: false
    property int lingerMs: 300
    property string heldScrollKey: ""
    property bool heldScrollShift: false
    property bool scrollBusy: false
    property bool scrollPassThrough: false
    property bool scrollReadySent: false
    property int scrollRepeatMs: 80
    property var deferredKey: null
    property var deferredEnd: null
    // Delay only an uppercase key that completes a visible label. The click
    // must not inherit the physical Shift modifier from the browser surface.
    property var pendingShiftActivation: null

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
    property color scrollAccent: "#a6e3a1"
    property color imageBackground: "#332a16"
    property color imageBorder: "#fab387"
    property string fontFamily: "monospace"

    signal send(var request)
    // Why the session ended, and whether thodud now acts: the overlay must
    // then hide and say so with a hidden request.
    signal finished(string why, bool acting)
    // thodud clicks while the view keeps the keyboard: the overlay must let
    // the pointer through and say it is out of the way.
    signal lingered
    // The input mask changes without changing painted pixels. Ask the owner
    // to schedule a frame so the pass-through region is committed before the
    // daemon sends the wheel event.
    signal scrollFrameRequested()

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
        view.labelSelected = false;
        view.search = "";
        view.labelsOnly = false;
        view.scrollMode = false;
        view.searching = false;
        view.labelPrefix = "";
        view.status = "";
        view.statusDisplay = "auto";
        view.statusPersistent = false;
        view.statusExpired = false;
        view.collecting = false;
        view.generation = 0;
        view.fetching = false;
        view.lingering = false;
        view.heldScrollKey = "";
        view.heldScrollShift = false;
        view.scrollBusy = false;
        view.scrollPassThrough = false;
        view.scrollReadySent = false;
        view.deferredKey = null;
        view.deferredEnd = null;
        view.pendingShiftActivation = null;
        view.statusTimeoutMs = 2000;
        view.scrollRepeatMs = 80;
        view.active = true;
        view.focus = true;
        silence.restart();
        statusExpiry.stop();
        scrollRepeat.stop();
        if (reply)
            view.apply(reply);
        else
            view.send({ op: "session", session: id });
    }

    function end(why, acting) {
        if (!view.active)
            return;
        if (view.scrollMode && view.scrollBusy && why !== "thodud stopped answering" &&
                why !== "thodud closed the connection" && why !== "escape") {
            view.stopScrollHold("");
            view.deferredEnd = { why: why, acting: acting };
            return;
        }
        view.active = false;
        view.focus = false;
        view.lingering = false;
        view.heldScrollKey = "";
        view.scrollBusy = false;
        view.scrollPassThrough = false;
        view.deferredKey = null;
        view.deferredEnd = null;
        view.pendingShiftActivation = null;
        silence.stop();
        linger.stop();
        statusExpiry.stop();
        scrollRepeat.stop();
        view.finished(why, acting);
    }

    function apply(reply) {
        if (!view.active || !reply)
            return;
        if (reply.session !== undefined && reply.session !== view.session)
            return;
        silence.restart();
        if (!reply.ok) {
            view.scrollBusy = false;
            view.scrollPassThrough = false;
            view.end(reply.message || "thodud refused the session", false);
            return;
        }
        if (reply.workspace_changed === true) {
            view.end("workspace changed", false);
            return;
        }
        const wasLoaded = view.loaded;
        const wasScrollPassThrough = view.scrollPassThrough;
        const previousStatus = view.status;
        const shownGeneration = reply.view ? (reply.view.generation || 0) : view.generation;
        if (reply.targets) {
            if (!view.loaded || shownGeneration !== view.generation) {
                view.targets = reply.targets;
                view.generation = shownGeneration;
                if (reply.monitor) {
                    view.area = reply.monitor.area;
                    view.monitorName = reply.monitor.name;
                }
                view.loaded = true;
            }
            view.fetching = false;
        } else if (view.loaded && shownGeneration !== view.generation && !view.fetching) {
            // Keys sent over the IPC changed the targets; fetch them.
            view.fetching = true;
            view.collecting = true;
            view.send({ op: "session", session: view.session });
        }
        if (reply.status)
            view.status = reply.status;
        const shown = reply.view;
        if (!shown)
            return;
        view.collecting = shown.collecting === true || view.fetching;
        view.statusDisplay = shown.status_display || "auto";
        view.statusPersistent = shown.status_persistent === true;
        const kept = {};
        for (const id of shown.visible)
            kept[id] = true;
        view.kept = kept;
        view.focusedId = shown.focused || "";
        view.labelSelected = shown.label_selected === true;
        view.labelsOnly = shown.labels_only === true;
        view.scrollMode = shown.scroll_mode === true;
        if (view.scrollMode && shown.state === "acting") {
            if (!view.scrollPassThrough)
                view.scrollReadySent = false;
            view.scrollBusy = true;
            view.scrollPassThrough = true;
            view.scrollFrameRequested();
        } else if (view.scrollMode && shown.state === "showing") {
            view.scrollBusy = false;
            view.scrollPassThrough = false;
            view.scrollReadySent = false;
            if (wasScrollPassThrough && reply.status && reply.status !== previousStatus)
                view.stopScrollHold("");
        }
        view.search = shown.search || "";
        view.searching = shown.searching === true;
        view.labelPrefix = shown.label_prefix || "";
        if (!wasLoaded && view.loaded)
            view.restartStatusExpiry();
        if (shown.state === "acting" && view.scrollMode) {
            // The layer remains mapped and keyboard-exclusive. Thodu waits
            // for notifyScrollReady after this frame passes wheel input through.
        } else if (shown.state === "acting" && shown.linger) {
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
        if (shown.state === "showing" && view.scrollMode && view.deferredEnd !== null) {
            const pending = view.deferredEnd;
            view.deferredEnd = null;
            view.end(pending.why, pending.acting);
        } else if (shown.state === "showing" && view.scrollMode && view.deferredKey !== null) {
            const pending = view.deferredKey;
            view.deferredKey = null;
            if (pending.key === Qt.Key_Escape) {
                view.end("escape", false);
            } else if (!view.collecting) {
                const request = view.keyRequest(pending.key, pending.modifiers, pending.text, pending.autoRepeat);
                if (request)
                    view.submitKeyRequest(request);
            }
        }
    }

    // A key as SPEC section 7 names it, or null for keys a session ignores.
    function keyRequest(key, modifiers, text, autoRepeat) {
        // Caps Lock can make text uppercase without Shift. Only Shift means
        // activation; repeats cannot complete labels or double-click.
        const shift = (modifiers & Qt.ShiftModifier) !== 0;
        if (view.scrollMode && autoRepeat)
            return null;
        const letter = key >= Qt.Key_A && key <= Qt.Key_Z;
        const labelInput = shift || view.labelPrefix.length > 0 || (view.labelsOnly && !view.searching);
        if (autoRepeat && (key === Qt.Key_Return || key === Qt.Key_Enter || (letter && labelInput)))
            return null;
        const ctrl = (modifiers & Qt.ControlModifier) !== 0;
        const enter = key === Qt.Key_Return || key === Qt.Key_Enter;
        // A chord is a shortcut meant for an app: only Escape, and Return
        // with Control for a Control click, keep their meaning.
        if (key !== Qt.Key_Escape && view.isChord(enter ? modifiers & ~Qt.ControlModifier : modifiers))
            return null;
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
        else if (view.scrollMode && !shift && key >= Qt.Key_0 && key <= Qt.Key_9)
            name = String.fromCharCode(key);
        else if (key >= Qt.Key_A && key <= Qt.Key_Z) {
            const lower = String.fromCharCode(key).toLowerCase();
            name = shift ? lower.toUpperCase() : lower;
        }
        else
            return null;
        return { op: "key", session: view.session, generation: view.generation, key: name, shift: shift, ctrl: ctrl };
    }

    // Control, Alt or Super make a key a shortcut meant for an app.
    function isChord(modifiers) {
        return (modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) !== 0;
    }

    function scrollKeyName(key) {
        if (key === Qt.Key_H) return "h";
        if (key === Qt.Key_J) return "j";
        if (key === Qt.Key_K) return "k";
        if (key === Qt.Key_L) return "l";
        return "";
    }

    function scrollKeyCode(name) {
        if (name === "h") return Qt.Key_H;
        if (name === "j") return Qt.Key_J;
        if (name === "k") return Qt.Key_K;
        if (name === "l") return Qt.Key_L;
        return Qt.Key_unknown;
    }

    function startScrollHold(name, shift) {
        if (!view.active || !view.scrollMode || view.targets.length === 0 || name.length !== 1)
            return;
        view.heldScrollKey = name;
        view.heldScrollShift = shift;
        if (view.scrollBusy)
            return;
        const request = view.keyRequest(view.scrollKeyCode(name), shift ? Qt.ShiftModifier : Qt.NoModifier, name, false);
        if (!request)
            return;
        view.scrollBusy = true;
        view.send(request);
    }

    function repeatScrollHold() {
        if (!view.active || !view.scrollMode || view.scrollBusy || view.heldScrollKey.length === 0)
            return;
        const request = view.keyRequest(view.scrollKeyCode(view.heldScrollKey),
                                        view.heldScrollShift ? Qt.ShiftModifier : Qt.NoModifier,
                                        view.heldScrollKey, false);
        if (!request)
            return;
        view.scrollBusy = true;
        view.send(request);
    }

    function stopScrollHold(name) {
        if (name.length > 0 && name !== view.heldScrollKey)
            return;
        const wasHeld = view.heldScrollKey.length > 0;
        view.heldScrollKey = "";
        view.heldScrollShift = false;
        if (wasHeld && view.active && view.scrollMode)
            view.send({ op: "scroll-stop", session: view.session });
    }

    function notifyScrollReady() {
        if (!view.active || !view.scrollPassThrough || view.scrollReadySent)
            return;
        view.scrollReadySent = true;
        view.send({ op: "scroll-ready", session: view.session });
    }

    function restartStatusExpiry() {
        view.statusExpired = false;
        statusExpiry.restart();
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
        if (typed > 0 && !label.toLowerCase().startsWith(view.labelPrefix.toLowerCase()))
            return label.toUpperCase();
        const head = label.slice(0, typed).toUpperCase();
        const rest = label.slice(typed).toUpperCase();
        if (!head)
            return rest;
        return "<font color=\"" + view.selectedText + "\">" + head + "</font>" + rest;
    }

    function inGrid() {
        return view.targets.length > 0 && view.targets[0].source === "grid";
    }

    function hintCue() {
        if (view.inGrid())
            return "";
        if (view.collecting)
            return "visual targets loading | Esc cancels";
        if (view.scrollMode) {
            if (view.targets.length === 0)
                return "no scroll areas | Esc exits";
            return "hold HJKL to scroll | Shift larger | Tab cycles | digits select | Esc exits";
        }
        if (view.pendingShiftActivation)
            return "release Shift to activate | Esc cancels";
        if (view.labelPrefix.length > 0)
            return "lowercase continues | uppercase final activates | Tab cycles hints";
        if (view.labelsOnly)
            return view.searching ? "lowercase searches | uppercase final activates | Tab cycles hints" :
                "lowercase selects | uppercase final activates | / searches | Tab cycles hints";
        return "type to search | Shift+hint activates | Tab cycles hints";
    }

    // Send the exact geometry Thodu paints so the screenshot detector can
    // ignore hint chips and the status toast without estimating QML layout.
    function overlayRects() {
        const rects = [];
        for (const target of view.targets) {
            if (view.kept[target.id] === true && !view.lingering &&
                    !(view.zoomed && target.source === "grid"))
                rects.push(target.placed);
        }
        if (focusRing.visible) {
            const x = view.area.x + focusRing.x;
            const y = view.area.y + focusRing.y;
            const right = x + focusRing.width;
            const bottom = y + focusRing.height;
            rects.push({
                x: Math.floor(x),
                y: Math.floor(y),
                w: Math.ceil(right) - Math.floor(x),
                h: Math.ceil(bottom) - Math.floor(y)
            });
        }
        if (statusLine.visible) {
            const x = view.area.x + statusLine.x;
            const y = view.area.y + statusLine.y;
            const right = x + statusLine.width;
            const bottom = y + statusLine.height;
            rects.push({
                x: Math.floor(x),
                y: Math.floor(y),
                w: Math.ceil(right) - Math.floor(x),
                h: Math.ceil(bottom) - Math.floor(y)
            });
        }
        return rects;
    }

    function drawnMessage() {
        return { op: "drawn", session: view.session, overlays: view.overlayRects() };
    }

    function shouldWaitForShiftRelease(request) {
        if (view.scrollMode || view.inGrid() || !request || !request.shift || request.key.length !== 1 ||
                request.key < "A" || request.key > "Z")
            return false;
        const candidate = (view.labelPrefix + request.key.toLowerCase()).toLowerCase();
        return view.targets.some(target => view.kept[target.id] === true &&
            typeof target.label === "string" && target.label.toLowerCase() === candidate);
    }

    function dispatchKeyRequest(request) {
        if (!view.active)
            return;
        view.send(request);
        // REQ-SESSION-02: Escape gives the keyboard back once dispatched.
        if (request.key === "Escape")
            view.end("escape", false);
        // After a click only a plain Return means anything: a double click.
        else if (view.lingering && (request.key !== "Return" || request.shift || request.ctrl))
            view.end("a key in the wait after a click", false);
    }

    function submitKeyRequest(request) {
        // Once a visible label is ready to activate, later keys must not pass
        // it and reach thodud after the activation. Escape cancels it.
        if (view.pendingShiftActivation) {
            if (request.key === "Escape") {
                view.pendingShiftActivation = null;
                view.dispatchKeyRequest(request);
            }
            return;
        }
        if (view.shouldWaitForShiftRelease(request)) {
            view.pendingShiftActivation = request;
            return;
        }
        view.dispatchKeyRequest(request);
    }

    function flushShiftActivation() {
        const request = view.pendingShiftActivation;
        view.pendingShiftActivation = null;
        if (view.active && request)
            view.dispatchKeyRequest(request);
    }

    Keys.onPressed: event => {
        event.accepted = true;
        if (!view.active)
            return;
        const scrollKey = view.scrollMode ? view.scrollKeyName(event.key) : "";
        if (scrollKey.length > 0) {
            if (view.isChord(event.modifiers))
                view.stopScrollHold("");
            else if (!event.isAutoRepeat)
                view.startScrollHold(scrollKey, (event.modifiers & Qt.ShiftModifier) !== 0);
            return;
        }
        const modifierKey = event.key === Qt.Key_Shift || event.key === Qt.Key_Control ||
            event.key === Qt.Key_Alt || event.key === Qt.Key_Meta;
        if (view.scrollMode && view.scrollBusy && !modifierKey && event.key !== Qt.Key_Escape) {
            view.stopScrollHold("");
            view.deferredKey = {
                key: event.key,
                modifiers: event.modifiers,
                text: event.text,
                autoRepeat: event.isAutoRepeat
            };
            return;
        }
        if (view.scrollMode && !modifierKey)
            view.stopScrollHold("");
        if (view.collecting && event.key !== Qt.Key_Escape)
            return;
        const request = view.keyRequest(event.key, event.modifiers, event.text, event.isAutoRepeat);
        if (!request) {
            // A chord in the wait after a click is no double click.
            if (view.lingering && !modifierKey && view.isChord(event.modifiers))
                view.end("a key in the wait after a click", false);
            return;
        }
        view.submitKeyRequest(request);
    }

    Keys.onReleased: event => {
        event.accepted = true;
        if (!view.active || event.isAutoRepeat)
            return;
        if (event.key === Qt.Key_Shift) {
            view.flushShiftActivation();
            return;
        }
        if (!view.scrollMode)
            return;
        const name = view.scrollKeyName(event.key);
        if (name.length > 0)
            view.stopScrollHold(name);
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

    Timer {
        id: statusExpiry
        interval: view.statusTimeoutMs
        onTriggered: view.statusExpired = true
    }

    Timer {
        id: scrollRepeat
        interval: view.scrollRepeatMs
        repeat: true
        running: view.active && view.scrollMode && view.heldScrollKey.length > 0
        onTriggered: view.repeatScrollHold()
    }

    // The focused target's bounds, for a search that has not narrowed to
    // one label yet.
    Rectangle {
        id: focusRing
        objectName: "focus-ring"
        readonly property var target: view.targets.find(t => t.id === view.focusedId) || null
        visible: view.loaded && target !== null && !view.labelSelected && !view.lingering
        x: target ? target.bounds.x - view.area.x - 2 : 0
        y: target ? target.bounds.y - view.area.y - 2 : 0
        width: target ? target.bounds.w + 4 : 0
        height: target ? target.bounds.h + 4 : 0
        color: "transparent"
        radius: 4
        border.width: 2
        border.color: view.scrollMode ? view.scrollAccent : view.selectedText
    }

    // The grid's cells (REQ-GRID-01). Below the monitor's level they are
    // too small for their labels, so the area is outlined and its cells are
    // drawn again, magnified, in a panel beside it.
    readonly property var gridCells: view.loaded && !view.lingering ? view.targets.filter(t => t.source === "grid") : []
    readonly property var gridArea: view.bounding(view.gridCells.map(t => t.bounds))
    readonly property bool zoomed: view.gridCells.length > 0 && view.gridArea.w * view.gridArea.h < view.area.w * view.area.h
    readonly property var scrollHole: {
        const target = view.targets.find(t => t.id === view.focusedId);
        if (!target)
            return { x: 0, y: 0 };
        return {
            x: target.bounds.x - view.area.x + Math.floor(target.bounds.w / 2) - 1,
            y: target.bounds.y - view.area.y + Math.floor(target.bounds.h / 2) - 1
        };
    }

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
            readonly property bool imageDerived: modelData.source === "image"
            objectName: "label-" + modelData.label
            z: focusedOne ? 1 : 0
            visible: view.kept[modelData.id] === true && !view.lingering && !(view.zoomed && modelData.source === "grid")
            x: modelData.placed.x - view.area.x
            y: modelData.placed.y - view.area.y
            width: modelData.placed.w
            height: modelData.placed.h
            radius: 3
            color: focusedOne ? view.selectedBackground : imageDerived ? view.imageBackground : view.background
            border.width: focusedOne ? 2 : 1
            border.color: focusedOne ? view.selectedText : imageDerived ? view.imageBorder : view.border

            Text {
                anchors.centerIn: parent
                text: view.labelText(chip.modelData.label)
                textFormat: Text.StyledText
                color: chip.focusedOne ? view.selectedText : chip.imageDerived ? view.imageBorder : view.foreground
                font.family: view.fontFamily
                font.pixelSize: 12
                font.bold: true
            }
        }
    }

    TextMetrics {
        id: statusMetrics
        text: statusText.text
        font: statusText.font
    }

    TextMetrics {
        id: hintMetrics
        text: hintText.text
        font: hintText.font
    }

    Rectangle {
        id: statusLine
        objectName: "status-line"
        readonly property real maxWidth: Math.max(1, Math.min(parent.width - 32, 640))
        visible: view.loaded && (statusText.shouldShow || hintText.text.length > 0) && !view.lingering
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 24
        width: Math.min(maxWidth, Math.max(statusText.shouldShow ? statusMetrics.width : 0,
                                            hintText.text.length > 0 ? hintMetrics.width : 0) + 24)
        height: statusContent.implicitHeight + 16
        radius: 6
        color: view.background
        border.width: 1
        border.color: view.border

        Column {
            id: statusContent
            anchors.centerIn: parent
            width: parent.width - 24
            spacing: 3

            Text {
                id: statusText
                objectName: "status-text"
                width: parent.width
                text: view.labelPrefix.length > 0 ? view.labelPrefix.toUpperCase() :
                    (view.search || view.searching ? "/" + view.search : view.status)
                readonly property bool shouldShow: text.length > 0 && (view.labelPrefix.length > 0 || view.search.length > 0 ||
                    view.searching || view.collecting || view.statusPersistent ||
                    view.statusDisplay === "always" ||
                    (view.statusDisplay === "auto" && !view.statusExpired))
                visible: shouldShow
                wrapMode: Text.Wrap
                color: view.foreground
                font.family: view.fontFamily
                font.pixelSize: 13
            }

            Text {
                id: hintText
                objectName: "hint-text"
                width: parent.width
                text: view.hintCue()
                visible: text.length > 0
                wrapMode: Text.Wrap
                color: view.selectedText
                font.family: view.fontFamily
                font.pixelSize: 11
            }
        }
    }
}

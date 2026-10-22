import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import "components"

// Thodu's overlay, loaded by omarchy-shell (SPEC section 4, item 2). Kept
// loaded, it listens on thodud's socket and thodud pushes each session to
// it; when nothing listens, thodud summons it with {"session", "socket",
// "test"} instead. It draws the session on the session's monitor, takes
// the keyboard and reports keys to thodud. With test set it leaves the
// keyboard alone, for live checks that send keys over the IPC.
Item {
    id: root

    property var shell: null
    property var manifest: null
    property bool opened: false
    property bool test: false
    property string session: ""
    property bool drawnSent: false
    readonly property string socketPath: Quickshell.env("XDG_RUNTIME_DIR") + "/thodu.sock"
    // Avoid a visibility binding loop through PanelWindow.screen.
    readonly property var sessionScreen: root.screenNamed(view.monitorName)

    Component.onCompleted: {
        listener.start(root.socketPath);
        client.connectTo(root.socketPath);
    }

    function open(payloadJson) {
        let payload = {};
        try {
            payload = JSON.parse(payloadJson || "{}");
        } catch (e) {
            console.warn("thodu: a summon that is not JSON");
            return;
        }
        if (!payload.session || !payload.socket) {
            console.warn("thodu: a summon needs a session and a socket; start one with thodu hints");
            return;
        }
        root.show(String(payload.session), payload.test === true, null, String(payload.socket));
        listener.start(String(payload.socket));
    }

    // Draw a session: `reply` is its full session reply when pushed.
    function show(session, test, reply, socket) {
        view.end("replaced", false);
        root.test = test;
        root.session = session;
        root.drawnSent = false;
        client.connectTo(socket);
        root.opened = true;
        view.begin(session, reply);
        view.forceActiveFocus();
    }

    function close() {
        view.end("closed by the shell", false);
    }

    function toggle() {
        if (root.opened)
            root.close();
    }

    function screenNamed(name) {
        for (const s of Quickshell.screens) {
            if (s.name === name)
                return s;
        }
        return null;
    }

    Listener {
        id: listener
        onOpened: event => root.show(String(event.session), event.test === true, event.reply, listener.path)
    }

    Client {
        id: client
        onReply: reply => view.apply(reply)
        onLost: view.end("thodud closed the connection", false)
    }

    // The click waits for the overlay to go (REQ-ACT-01): the surface is
    // unmapped when the window hides, and thodud hears a moment later.
    Timer {
        id: hidden
        property string session: ""
        interval: 40
        onTriggered: client.send({ op: "hidden", session: hidden.session })
    }

    // The first frame with the labels, for REQ-PERF-01's measurement.
    Connections {
        target: view.Window.window
        ignoreUnknownSignals: true

        function onFrameSwapped() {
            if (!root.opened || !view.loaded || root.drawnSent)
                return;
            root.drawnSent = true;
            client.send({ op: "drawn", session: view.session });
        }
    }

    PanelWindow {
        id: panel
        visible: root.opened && view.loaded && root.sessionScreen !== null
        screen: root.sessionScreen
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        color: "transparent"
        // While it waits for a second Return after a click, the pointer
        // passes through to the window beneath (REQ-ACT-02).
        mask: view.lingering ? passThrough : null
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "thodu"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: view.active && !root.test ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

        Region {
            id: passThrough
        }

        // A click anywhere ends the session without acting.
        MouseArea {
            anchors.fill: parent
            onClicked: view.end("clicked away", false)
        }

        SessionView {
            id: view
            anchors.fill: parent
            background: Color.menu.background
            foreground: Color.menu.text
            border: Color.menu.border
            selectedBackground: Color.menu.selectedBackground
            selectedText: Color.menu.selectedText
            fontFamily: Style.font.family

            onSend: request => client.send(request)
            onLingered: {
                hidden.session = view.session;
                hidden.restart();
            }
            onFinished: (why, acting) => {
                const id = view.session;
                root.opened = false;
                if (acting) {
                    hidden.session = id;
                    hidden.restart();
                } else if (why !== "thodud stopped answering" && why !== "thodud closed the connection") {
                    client.send({ op: "close", session: id });
                }
            }
        }
    }
}

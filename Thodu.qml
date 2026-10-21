import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import "components"

// Thodu's overlay, loaded by omarchy-shell (SPEC section 4, item 2). thodud
// summons it with {"session", "socket", "test"}; it draws that session on
// the session's monitor, takes the keyboard and reports keys to thodud.
// With test set it leaves the keyboard alone, for live checks that send
// keys over the IPC.
Item {
    id: root

    property var shell: null
    property var manifest: null
    property bool opened: false
    property bool test: false
    property string session: ""

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
        view.end("replaced", false);
        root.test = payload.test === true;
        root.session = String(payload.session);
        client.connectTo(String(payload.socket));
        root.opened = true;
        view.begin(root.session);
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

    PanelWindow {
        id: panel
        visible: root.opened && view.loaded && screen !== null
        screen: root.screenNamed(view.monitorName)
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "thodu"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: view.active && !root.test ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

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

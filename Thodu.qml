import QtQuick

// Thodu's entry point, loaded by omarchy-shell. The shell sets
// shell and manifest, and calls open, close and toggle.
Item {
    id: root

    property var shell: null
    property var manifest: null
    property bool opened: false

    function open(payloadJson) {
        root.opened = true;
    }

    function close() {
        root.opened = false;
    }

    function toggle() {
        if (root.opened)
            root.close();
        else
            root.open("{}");
    }
}

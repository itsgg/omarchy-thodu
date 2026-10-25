import QtQuick

// Stands in for Omarchy's BorderSurface: a surface with no border width.
Item {
    property var borderSpec: null
    property color color: "transparent"
    property real radius: 0
    readonly property real borderLeft: 0
    readonly property real borderRight: 0
}

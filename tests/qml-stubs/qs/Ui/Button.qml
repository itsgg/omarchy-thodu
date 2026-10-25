import QtQuick

// Stands in for Omarchy's Button: its properties, a control's height, and a
// width that fits its icon and text.
Item {
    property string text: ""
    property string iconText: ""
    property bool leftAlign: false
    property bool hasCursor: false
    property bool bordered: false
    property real fontSize: 14
    property real iconSize: 16
    property real verticalPadding: 6
    signal clicked()
    signal hovered(bool isHovered)

    implicitWidth: 28 + text.length * 8
    implicitHeight: 28
}

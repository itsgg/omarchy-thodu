import QtQuick

// Stands in for Omarchy's Toggle row: Toggle's minimum height.
Item {
    property string label: ""
    property string description: ""
    property bool checked: false
    property bool hasCursor: false
    signal clicked()
    signal hovered(bool isHovered)

    implicitHeight: 54
}

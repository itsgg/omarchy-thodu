import QtQuick

// Stands in for Omarchy's ButtonGroup: one cell per option.
Item {
    property var options: []
    property string value: ""
    property bool focusable: true
    property int cursorIndex: -1
    property real fontSize: 14
    signal changed(string value)

    width: options.length * 56
    height: 28
}

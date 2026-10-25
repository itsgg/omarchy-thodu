import QtQuick

Item {
    id: root
    default property alias contentItem: content.data
    property Item anchorItem: null
    property QtObject bar: null
    property var owner: null
    property bool open: false
    property Item focusTarget: null
    property int padding: 0
    property int contentWidth: 0
    property int contentHeight: 0

    function fittedContentWidth(value) { return value }
    function fittedContentHeight(value) { return value }

    Item {
        id: content
        anchors.fill: parent
        width: root.contentWidth
        height: root.contentHeight
    }
}

import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "../lib/BarMenuActions.js" as Actions

Item {
    id: root

    property QtObject bar: null
    property Item anchorItem: null
    property var owner: null
    property bool open: false
    property string serviceState: "checking"
    property var options: ({})
    property bool settingsReady: false
    property bool configEditable: false
    property string configPath: ""
    property int selectedIndex: 0

    signal actionRequested(var action)

    readonly property int rowCount: 10
    readonly property string serviceLabel: serviceState === "active" ? "Ready"
        : serviceState === "checking" ? "Checking service" : "Service stopped"
    readonly property color serviceColor: serviceState === "active" ? Commons.Color.accent
        : serviceState === "checking" ? Commons.Color.foreground : Commons.Color.urgent

    function close() {
        root.open = false;
    }

    function dispatch(index, direction) {
        const action = Actions.actionFor(index, root.options, direction || 0);
        if (Actions.closesAfter(action))
            root.close();
        root.actionRequested(action);
    }

    function activateSelected() {
        dispatch(root.selectedIndex, 0);
    }

    function moveSelection(dx, dy) {
        if (dx !== 0) {
            const direction = dx < 0 ? -1 : 1;
            if (root.selectedIndex <= 2) {
                root.selectedIndex = (root.selectedIndex + direction + 3) % 3;
            } else if (root.selectedIndex === 3 || root.selectedIndex === 4) {
                dispatch(root.selectedIndex, direction);
            } else if (root.selectedIndex === 5 || root.selectedIndex === 6) {
                root.selectedIndex = direction < 0 ? 5 : 6;
            } else if (root.selectedIndex === 7 || root.selectedIndex === 8) {
                root.selectedIndex = direction < 0 ? 7 : 8;
            }
            return;
        }
        const direction = dy < 0 ? -1 : 1;
        root.selectedIndex = (root.selectedIndex + direction + root.rowCount) % root.rowCount;
    }

    onOpenChanged: if (open) selectedIndex = 0

    KeyboardPanel {
        id: panel
        objectName: "thoduBarMenuSurface"
        property bool thoduScanSkip: true

        anchorItem: root.anchorItem
        owner: root.owner
        bar: root.bar
        open: root.open
        focusTarget: keyCatcher
        padding: Style.space(14)
        contentWidth: panel.fittedContentWidth(Style.space(390))
        contentHeight: panel.fittedContentHeight(menuContent.implicitHeight)

        PanelKeyCatcher {
            id: keyCatcher
            anchors.fill: parent
            onMoveRequested: function(dx, dy) { root.moveSelection(dx, dy) }
            onActivateRequested: root.activateSelected()
            onCloseRequested: root.close()
            onTabRequested: function(direction) { root.moveSelection(0, direction) }
        }

        Column {
            id: menuContent
            anchors.fill: parent
            spacing: Style.space(8)

            Row {
                width: parent.width
                spacing: Style.space(8)
                Text {
                    text: "Thodu"
                    color: Commons.Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.body
                    font.bold: true
                    anchors.verticalCenter: parent.verticalCenter
                }
                Rectangle {
                    width: Style.space(7)
                    height: width
                    radius: width / 2
                    color: root.serviceColor
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    text: root.serviceLabel
                    color: root.serviceColor
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            Row {
                width: parent.width
                spacing: Style.space(6)

                Button {
                    width: (parent.width - Style.space(12)) / 3
                    text: "Hints"
                    hasCursor: root.selectedIndex === 0
                    onClicked: root.dispatch(0, 0)
                }
                Button {
                    width: (parent.width - Style.space(12)) / 3
                    text: "Grid"
                    hasCursor: root.selectedIndex === 1
                    onClicked: root.dispatch(1, 0)
                }
                Button {
                    width: (parent.width - Style.space(12)) / 3
                    text: "Scroll"
                    hasCursor: root.selectedIndex === 2
                    onClicked: root.dispatch(2, 0)
                }
            }

            PanelSeparator { width: parent.width }

            Button {
                width: parent.width
                text: "Labels only: " + (root.options.labels_only ? "On" : "Off")
                leftAlign: true
                hasCursor: root.selectedIndex === 3
                enabled: root.settingsReady
                onClicked: root.dispatch(3, 0)
            }

            Button {
                width: parent.width
                text: "Status detail: " + String(root.options.status_display || "auto")
                leftAlign: true
                hasCursor: root.selectedIndex === 4
                enabled: root.settingsReady
                onClicked: root.dispatch(4, 0)
            }

            Row {
                width: parent.width
                spacing: Style.space(6)
                Text {
                    width: parent.width - decrease.width - increase.width - parent.spacing * 2
                    text: "Scroll step: " + String(root.options.scroll_step || 1)
                    color: Commons.Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.body
                    verticalAlignment: Text.AlignVCenter
                }
                Button {
                    id: decrease
                    width: Style.space(32)
                    text: "-"
                    hasCursor: root.selectedIndex === 5
                    enabled: root.settingsReady
                    onClicked: root.dispatch(5, 0)
                }
                Button {
                    id: increase
                    width: Style.space(32)
                    text: "+"
                    hasCursor: root.selectedIndex === 6
                    enabled: root.settingsReady
                    onClicked: root.dispatch(6, 0)
                }
            }

            Row {
                width: parent.width
                spacing: Style.space(6)
                Text {
                    width: parent.width - decreaseShift.width - increaseShift.width - parent.spacing * 2
                    text: "Shift step: " + String(root.options.scroll_shift_step || 5)
                    color: Commons.Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.body
                    verticalAlignment: Text.AlignVCenter
                }
                Button {
                    id: decreaseShift
                    width: Style.space(32)
                    text: "-"
                    hasCursor: root.selectedIndex === 7
                    enabled: root.settingsReady
                    onClicked: root.dispatch(7, 0)
                }
                Button {
                    id: increaseShift
                    width: Style.space(32)
                    text: "+"
                    hasCursor: root.selectedIndex === 8
                    enabled: root.settingsReady
                    onClicked: root.dispatch(8, 0)
                }
            }

            Button {
                width: parent.width
                text: root.settingsReady ? "Edit configuration" : "Edit configuration to fix settings"
                leftAlign: true
                hasCursor: root.selectedIndex === 9
                enabled: root.configEditable
                onClicked: root.dispatch(9, 0)
            }

            Text {
                width: parent.width
                text: root.settingsReady ? "Changes apply to new sessions. Esc closes."
                    : "Configuration needs editing. Esc closes."
                color: Qt.darker(Commons.Color.foreground, 1.3)
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                wrapMode: Text.WordWrap
            }
        }
    }
}

pragma ComponentBehavior: Bound

import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "../lib/BarMenuActions.js" as Actions

// The bar control's menu (REQ-SHELL-02), laid out with Omarchy's own panel
// kit so it reads like the built-in panels: a header with the service state,
// the three session modes as bordered buttons, then labelled setting rows
// (title and a one-line description on the left, the control on the right)
// under a section header, and the editor action. Every setting row shares
// Toggle's metrics, so labels and controls line up down the panel.
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
    readonly property var statusValues: ["auto", "always", "off"]
    readonly property string statusValue: String(root.options.status_display || "auto")
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

    // A click on one option of a choice sets it directly; the keyboard steps.
    function choose(key, value) {
        root.actionRequested({ kind: "setting", key: key, value: value });
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
        contentWidth: panel.fittedContentWidth(Style.space(400))
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
            objectName: "thoduBarMenuContent"
            anchors.fill: parent
            spacing: Style.space(12)

            // ---------- Header: the name and whether thodud is ready ----------
            Item {
                id: header
                width: parent.width
                implicitHeight: Math.max(title.implicitHeight, service.implicitHeight)

                Text {
                    id: title
                    textFormat: Text.PlainText
                    text: "Thodu"
                    color: Commons.Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.title
                    font.bold: true
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                }

                Row {
                    id: service
                    objectName: "thoduBarMenuService"
                    spacing: Style.space(6)
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter

                    Rectangle {
                        width: Style.space(7)
                        height: width
                        radius: width / 2
                        color: root.serviceColor
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Text {
                        textFormat: Text.PlainText
                        text: root.serviceLabel.toUpperCase()
                        color: root.serviceColor
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                        font.bold: true
                        font.letterSpacing: 1.2
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }

            // ---------- The session modes ----------
            Row {
                id: modes
                width: parent.width
                spacing: Style.space(6)

                readonly property real cellWidth: (width - spacing * 2) / 3

                ModeButton { index: 0; iconText: ""; text: "Hints" }
                ModeButton { index: 1; iconText: ""; text: "Grid" }
                ModeButton { index: 2; iconText: ""; text: "Scroll" }
            }

            PanelSeparator { width: parent.width }

            Column {
                id: settings
                width: parent.width
                spacing: Style.space(8)

                PanelSectionHeader { text: "SETTINGS" }

                Toggle {
                    id: labelsOnly
                    objectName: "thoduSettingLabelsOnly"
                    width: parent.width
                    label: "Labels only"
                    description: "Letters type labels; / starts a search."
                    checked: root.options.labels_only === true
                    hasCursor: root.selectedIndex === 3
                    enabled: root.settingsReady
                    opacity: enabled ? 1 : 0.5
                    onClicked: root.dispatch(3, 0)
                    onHovered: function(on) { if (on) root.selectedIndex = 3 }
                }

                SettingRow {
                    objectName: "thoduSettingStatus"
                    label: "Status detail"
                    description: "Counts in the toast."
                    hasCursor: root.selectedIndex === 4
                    onHovered: function(on) { if (on) root.selectedIndex = 4 }

                    ButtonGroup {
                        options: root.statusValues
                        value: root.statusValue
                        focusable: false
                        cursorIndex: root.selectedIndex === 4 ? root.statusValues.indexOf(root.statusValue) : -1
                        fontSize: Style.font.bodySmall
                        enabled: root.settingsReady
                        onChanged: function(value) { root.choose("status_display", value) }
                    }
                }

                SettingRow {
                    objectName: "thoduSettingScrollStep"
                    label: "Scroll step"
                    description: "Wheel detents per H/J/K/L."
                    hasCursor: root.selectedIndex === 5 || root.selectedIndex === 6
                    onHovered: function(on) { if (on && root.selectedIndex !== 6) root.selectedIndex = 5 }

                    Stepper {
                        value: Number(root.options.scroll_step || 1)
                        downIndex: 5
                        upIndex: 6
                    }
                }

                SettingRow {
                    objectName: "thoduSettingShiftStep"
                    label: "Shift step"
                    description: "Wheel detents per Shift+H/J/K/L."
                    hasCursor: root.selectedIndex === 7 || root.selectedIndex === 8
                    onHovered: function(on) { if (on && root.selectedIndex !== 8) root.selectedIndex = 7 }

                    Stepper {
                        value: Number(root.options.scroll_shift_step || 5)
                        downIndex: 7
                        upIndex: 8
                    }
                }
            }

            PanelSeparator { width: parent.width }

            // ---------- The editor, and what applies when ----------
            Item {
                id: footer
                width: parent.width
                implicitHeight: Math.max(editConfig.implicitHeight, hint.implicitHeight)

                Button {
                    id: editConfig
                    objectName: "thoduEditConfiguration"
                    iconText: ""
                    text: root.settingsReady ? "Edit configuration" : "Edit to fix settings"
                    fontSize: Style.font.bodySmall
                    bordered: true
                    hasCursor: root.selectedIndex === 9
                    enabled: root.configEditable
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: root.dispatch(9, 0)
                    onHovered: function(on) { if (on) root.selectedIndex = 9 }
                }

                Text {
                    id: hint
                    textFormat: Text.PlainText
                    text: root.settingsReady ? "Applies to new sessions" : "The configuration needs editing"
                    color: Qt.darker(Commons.Color.foreground, 1.4)
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                    horizontalAlignment: Text.AlignRight
                    elide: Text.ElideLeft
                    anchors.left: editConfig.right
                    anchors.leftMargin: Style.space(10)
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
    }

    // A session mode: a bordered button with its icon, one of three across.
    component ModeButton: Button {
        required property int index
        width: modes.cellWidth
        fontSize: Style.font.bodySmall
        iconSize: Style.font.title
        bordered: true
        verticalPadding: Style.spacing.controlPaddingY + Style.space(2)
        hasCursor: root.selectedIndex === index
        onClicked: root.dispatch(index, 0)
        onHovered: function(on) { if (on) root.selectedIndex = index }
    }

    // A labelled setting with Toggle's metrics, surface and cursor styling:
    // the title and description on the left, the control on the right.
    component SettingRow: BorderSurface {
        id: row
        property string label: ""
        property string description: ""
        property bool hasCursor: false
        default property alias control: slot.data
        readonly property Item labelArea: labels
        readonly property Item controlArea: slot
        signal hovered(bool on)

        readonly property bool hot: hasCursor || hover.hovered

        width: parent ? parent.width : implicitWidth
        implicitHeight: Math.max(54, labels.implicitHeight + Style.spacing.huge, slot.height + Style.spacing.huge)
        radius: Style.cornerRadius
        color: Style.controlFill(false, hot, Commons.Color.foreground, Commons.Color.accent)
        borderSpec: Border.controlSpec(hot ? "hover-cursor" : "normal", Commons.Color.foreground, Commons.Color.accent)

        Behavior on color { ColorAnimation { duration: 100 } }

        Column {
            id: labels
            anchors.left: parent.left
            anchors.leftMargin: row.borderLeft + Style.spacing.rowPaddingX
            anchors.right: slot.left
            anchors.rightMargin: Style.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.spacing.xs

            Text {
                textFormat: Text.PlainText
                text: row.label
                color: Commons.Color.foreground
                font.family: Style.font.family
                font.pixelSize: Style.font.subtitle
                font.bold: true
                elide: Text.ElideRight
                width: parent.width
            }
            Text {
                textFormat: Text.PlainText
                visible: row.description !== ""
                text: row.description
                color: Qt.darker(Commons.Color.foreground, 1.5)
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                wrapMode: Text.WordWrap
                width: parent.width
            }
        }

        Item {
            id: slot
            anchors.right: parent.right
            anchors.rightMargin: row.borderRight + Style.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            width: childrenRect.width
            height: childrenRect.height
        }

        HoverHandler {
            id: hover
            onHoveredChanged: row.hovered(hovered)
        }
    }

    // A number with a minus and a plus button around it; each button is its
    // own cursor stop, so Left and Right move between them and Enter steps.
    component Stepper: Row {
        id: stepper
        property int value: 1
        property int downIndex: 0
        property int upIndex: 0

        spacing: Style.space(4)

        Button {
            width: Style.spacing.controlHeight
            iconText: ""
            bordered: true
            hasCursor: root.selectedIndex === stepper.downIndex
            enabled: root.settingsReady && stepper.value > 1
            anchors.verticalCenter: parent.verticalCenter
            onClicked: root.dispatch(stepper.downIndex, 0)
            onHovered: function(on) { if (on) root.selectedIndex = stepper.downIndex }
        }
        Text {
            textFormat: Text.PlainText
            text: String(stepper.value)
            width: Style.space(28)
            horizontalAlignment: Text.AlignHCenter
            color: Commons.Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            font.bold: true
            anchors.verticalCenter: parent.verticalCenter
        }
        Button {
            width: Style.spacing.controlHeight
            iconText: ""
            bordered: true
            hasCursor: root.selectedIndex === stepper.upIndex
            enabled: root.settingsReady && stepper.value < 100
            anchors.verticalCenter: parent.verticalCenter
            onClicked: root.dispatch(stepper.upIndex, 0)
            onHovered: function(on) { if (on) root.selectedIndex = stepper.upIndex }
        }
    }
}

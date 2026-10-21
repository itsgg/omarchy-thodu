import QtQuick
import QtTest
import "../../lib/BarScan.js" as BarScan

// REQ-SHELL-01: a fake bar of three items, two WidgetButton-like click
// targets and a tray icon's MouseArea, under the slot pointer that every
// bar module has, reports three targets with their bounds.
Item {
    id: bar
    width: 1536
    height: 26

    // Like Omarchy's WidgetButton, with the MouseArea it takes clicks by.
    component FakeButton: Item {
        property string text: ""
        property string tooltipText: ""
        property bool interactive: true
        property bool pressable: true
        property bool concealed: false
        function triggerPress(button) {}
        MouseArea {
            anchors.fill: parent
        }
    }

    // What Bar.qml's ModuleSlot lays over each module.
    component SlotPointer: MouseArea {
        property bool suppressClick: false
        property real dragThreshold: 4
        anchors.fill: parent
    }

    Row {
        x: 10
        y: 2

        Item {
            width: 60
            height: 22
            FakeButton { objectName: "one"; text: "1"; width: 30; height: 22 }
            FakeButton { x: 30; text: "2"; width: 30; height: 22; tooltipText: "Workspace 2" }
            SlotPointer {}
        }

        Item {
            width: 24
            height: 22
            property string moduleName: "omarchy.tray"
            MouseArea { x: 2; y: 1; width: 20; height: 20 }
            SlotPointer {}
        }

        Item {
            width: 100
            height: 22
            // A module with nothing to click: only its slot pointer.
            Text { text: "static" }
            SlotPointer {}
        }

        Item {
            width: 40
            height: 22
            FakeButton { text: "hidden"; width: 40; height: 22; visible: false }
            FakeButton { text: "concealed"; width: 40; height: 22; concealed: true }
        }
    }

    Item {
        id: probe
        visible: false
    }

    TestCase {
        name: "BarScan"
        when: windowShown

        function test_req_shell_01_a_fake_bar_reports_its_three_items() {
            const found = BarScan.scan(BarScan.rootOf(probe), probe);
            compare(found.length, 3, JSON.stringify(found));
            compare(found[0], { x: 10, y: 2, w: 30, h: 22, text: "1" });
            compare(found[1], { x: 40, y: 2, w: 30, h: 22, text: "Workspace 2" }, "the tooltip before a glyph");
            compare(found[2], { x: 72, y: 3, w: 20, h: 20, text: "omarchy.tray" });
        }

        function test_req_shell_01_the_root_is_the_bar_window_content() {
            compare(BarScan.rootOf(findChild(bar, "one")), BarScan.rootOf(probe));
        }
    }
}

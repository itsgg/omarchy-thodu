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
            compare(Object.keys(found[0]).sort(), ["h", "identity", "text", "w", "x", "y"]);
            compare([found[0].x, found[0].y, found[0].w, found[0].h, found[0].text], [10, 2, 30, 22, "1"]);
            compare([found[1].x, found[1].y, found[1].w, found[1].h, found[1].text], [40, 2, 30, 22, "Workspace 2"], "the tooltip before a glyph");
            compare([found[2].x, found[2].y, found[2].w, found[2].h, found[2].text], [72, 3, 20, 20, "omarchy.tray"]);
            verify(found.every(item => item.identity.length > 0));
        }

        function test_req_shell_01_item_identity_is_stable_and_distinguishes_replacements() {
            const original = { text: "Clock 1:30" };
            const identity = BarScan.identityOf(original);
            original.text = "Clock 1:31";
            compare(BarScan.identityOf(original), identity);
            verify(BarScan.identityOf({ text: "Clock 1:31" }) !== identity);

            const firstScan = BarScan.scan(BarScan.rootOf(probe), probe);
            const secondScan = BarScan.scan(BarScan.rootOf(probe), probe);
            compare(firstScan[0].identity, secondScan[0].identity);
        }

        function test_req_shell_01_the_root_is_the_bar_window_content() {
            compare(BarScan.rootOf(findChild(bar, "one")), BarScan.rootOf(probe));
        }
    }
}

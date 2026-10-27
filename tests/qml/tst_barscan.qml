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

        function test_req_shell_01_identity_cache_evicts_old_unused_items() {
            const oldItem = { text: "Old target" };
            const oldIdentity = BarScan.identityOf(oldItem);
            for (let i = 0; i < 1100; i++)
                BarScan.identityOf({ index: i });
            verify(BarScan.identityOf(oldItem) !== oldIdentity);
        }

        function test_req_shell_01_active_identity_survives_cache_churn() {
            const activeItem = { text: "Active target" };
            const activeIdentity = BarScan.identityOf(activeItem);
            for (let i = 0; i < 1100; i++) {
                BarScan.identityOf({ index: i });
                BarScan.identityOf(activeItem);
            }
            compare(BarScan.identityOf(activeItem), activeIdentity);
        }

        function test_req_shell_01_scan_reaches_a_deep_bar_target() {
            function node(parent) {
                return {
                    parent: parent,
                    children: [],
                    visible: true,
                    opacity: 1,
                    width: 10,
                    height: 10,
                    mapToItem: function(root, x, y) { return { x: x, y: y } }
                };
            }

            const tree = node(null);
            let at = tree;
            for (let depth = 0; depth < 80; depth++) {
                const child = node(at);
                at.children.push(child);
                at = child;
            }
            at.text = "Deep target";
            at.triggerPress = function(button) {};

            const found = BarScan.scan(tree, null);
            compare(found.length, 1, JSON.stringify(found));
            compare(found[0].text, "Deep target");
        }

        function test_req_shell_01_scan_visits_cyclic_bar_items_once() {
            function node(parent) {
                return {
                    parent: parent,
                    children: [],
                    visible: true,
                    opacity: 1,
                    width: 10,
                    height: 10,
                    mapToItem: function(root, x, y) { return { x: x, y: y } }
                };
            }

            const tree = node(null);
            const target = node(tree);
            target.text = "One target";
            target.triggerPress = function(button) {};
            tree.children.push(tree, target);

            const found = BarScan.scan(tree, null);
            compare(found.length, 1, JSON.stringify(found));
            compare(found[0].text, "One target");
        }

        function test_req_shell_01_the_root_is_the_bar_window_content() {
            compare(BarScan.rootOf(findChild(bar, "one")), BarScan.rootOf(probe));
        }
    }
}

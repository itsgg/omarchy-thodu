import QtQuick
import QtTest
import "../../lib/BarScan.js" as BarScan

// REQ-SHELL-02: transient popup actions are not bar targets.
Item {
    id: desktop
    width: 300
    height: 100

    component FakeButton: Item {
        property string text: ""
        property string tooltipText: ""
        property bool interactive: true
        property bool pressable: true
        property bool concealed: false
        function triggerPress(button) {}
    }

    Item {
        id: bar
        width: 280
        height: 30
        FakeButton {
            objectName: "bar-control"
            text: "workspace"
            width: 40
            height: 24
        }
        Item {
            objectName: "thodu-menu"
            property bool thoduScanSkip: true
            x: 60
            width: 200
            height: 60
            FakeButton { text: "Hints"; width: 60; height: 24 }
        }
    }

    Item {
        id: probe
        visible: false
    }

    TestCase {
        name: "ThoduBarMenuScan"
        when: windowShown

        function test_req_shell_02_popup_controls_are_not_reported_as_bar_targets() {
            const found = BarScan.scan(BarScan.rootOf(probe), probe);
            compare(found.length, 1, JSON.stringify(found));
            compare(found[0].text, "workspace");
        }
    }
}

import QtQuick
import QtTest
import "../../components" as Components

// REQ-SHELL-02: the menu is laid out like Omarchy's own panels. Every setting
// row shares one left edge for its title and one right edge for its control,
// controls sit centred in their rows, and the modes share the width.
Item {
    id: desktop
    width: 600
    height: 600

    Item { id: anchor; width: 24; height: 24 }

    QtObject {
        id: bar
        property string position: "top"
        property int barSize: 32
    }

    Components.BarMenu {
        id: menu
        anchorItem: anchor
        bar: bar
        owner: menu
        options: ({ labels_only: false, status_display: "always", scroll_step: 1, scroll_shift_step: 5 })
        settingsReady: true
        configEditable: true
    }

    SignalSpy {
        id: actions
        target: menu
        signalName: "actionRequested"
    }

    TestCase {
        name: "ThoduBarMenuLayout"
        when: windowShown

        function init() {
            menu.open = true;
            menu.selectedIndex = 0;
            actions.clear();
        }

        function rows() {
            return ["thoduSettingStatus", "thoduSettingScrollStep", "thoduSettingShiftStep"].map(function(name) {
                const row = findChild(menu, name);
                verify(row !== null, name + " exists");
                return row;
            });
        }

        function edgeLeft(item) { return item.mapToItem(desktop, 0, 0).x; }
        function edgeRight(item) { return item.mapToItem(desktop, item.width, 0).x; }
        function midY(item) { return item.mapToItem(desktop, 0, item.height / 2).y; }

        function test_setting_rows_share_their_edges_with_the_toggle_row() {
            const toggle = findChild(menu, "thoduSettingLabelsOnly");
            verify(toggle !== null);
            const all = rows();
            for (let i = 0; i < all.length; i++) {
                compare(edgeLeft(all[i]), edgeLeft(toggle), all[i].objectName + " starts where the toggle row does");
                compare(edgeRight(all[i]), edgeRight(toggle), all[i].objectName + " ends where the toggle row does");
            }
        }

        function test_titles_share_a_left_edge_and_controls_a_right_edge() {
            const all = rows();
            for (let i = 1; i < all.length; i++) {
                compare(edgeLeft(all[i].labelArea), edgeLeft(all[0].labelArea), "titles line up");
                compare(edgeRight(all[i].controlArea), edgeRight(all[0].controlArea), "controls line up");
            }
            verify(edgeRight(all[0].controlArea) < edgeRight(all[0]), "the control keeps the row's padding");
            verify(edgeLeft(all[0].labelArea) > edgeLeft(all[0]), "the title keeps the row's padding");
        }

        function test_controls_sit_centred_in_their_rows() {
            const all = rows();
            for (let i = 0; i < all.length; i++) {
                fuzzyCompare(midY(all[i].controlArea), midY(all[i]), 1, all[i].objectName + " control centred");
                fuzzyCompare(midY(all[i].labelArea), midY(all[i]), 1, all[i].objectName + " title centred");
                verify(edgeRight(all[i].labelArea) <= edgeLeft(all[i].controlArea), all[i].objectName + " title clear of its control");
            }
        }

        function test_the_modes_share_the_width() {
            const content = findChild(menu, "thoduBarMenuContent");
            verify(content !== null);
            const modes = content.children[1];
            compare(modes.children.length, 3);
            const width = modes.children[0].width;
            for (let i = 1; i < 3; i++)
                compare(modes.children[i].width, width);
            fuzzyCompare(edgeRight(modes.children[2]), edgeRight(content), 0.5, "the modes fill the row");
        }

        function test_the_status_choice_shows_its_value_and_sets_a_clicked_one() {
            const group = rows()[0].controlArea.children[0];
            compare(group.value, "always");
            compare(group.cursorIndex, -1, "no cursor until the row is selected");
            menu.selectedIndex = 4;
            compare(group.cursorIndex, 1, "the cursor marks the current value");
            group.changed("off");
            compare(actions.count, 1);
            compare(actions.signalArguments[0][0].key, "status_display");
            compare(actions.signalArguments[0][0].value, "off");
            verify(menu.open, "a setting keeps the menu open");
        }

        function test_a_step_at_its_floor_cannot_go_lower() {
            const stepper = rows()[1].controlArea.children[0];
            verify(!stepper.children[0].enabled, "minus is off at 1");
            verify(stepper.children[2].enabled, "plus is on");
            const shift = rows()[2].controlArea.children[0];
            verify(shift.children[0].enabled, "minus is on at 5");
        }
    }
}

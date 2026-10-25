import QtQuick
import QtTest
import "../../components" as Components

// REQ-SHELL-02: the menu navigates by keyboard and dispatches each session mode.
Item {
    id: desktop
    width: 600
    height: 400

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
        options: ({ labels_only: false, status_display: "auto", scroll_step: 1, scroll_shift_step: 5 })
        settingsReady: true
        configEditable: true
    }

    SignalSpy {
        id: actions
        target: menu
        signalName: "actionRequested"
    }

    TestCase {
        name: "ThoduBarMenuComponent"
        when: windowShown

        function init() {
            menu.open = true;
            menu.selectedIndex = 0;
            actions.clear();
        }

        function test_menu_dispatches_each_mode_and_closes() {
            const modes = ["hints", "grid", "scroll"];
            for (let i = 0; i < modes.length; i++) {
                menu.open = true;
                menu.selectedIndex = i;
                actions.clear();
                menu.activateSelected();
                compare(actions.count, 1);
                compare(actions.signalArguments[0][0].kind, "launch");
                compare(actions.signalArguments[0][0].mode, modes[i]);
                verify(!menu.open, modes[i] + " closes the menu");
            }
        }

        function test_horizontal_navigation_moves_across_mode_buttons() {
            menu.moveSelection(1, 0);
            compare(menu.selectedIndex, 1);
            compare(actions.count, 0, "moving focus must not launch a mode");
            menu.moveSelection(1, 0);
            compare(menu.selectedIndex, 2);
            menu.moveSelection(1, 0);
            compare(menu.selectedIndex, 0, "the mode row wraps");
        }

        function test_horizontal_navigation_changes_stepper_focus_not_settings() {
            menu.selectedIndex = 5;
            menu.moveSelection(1, 0);
            compare(menu.selectedIndex, 6);
            compare(actions.count, 0, "arrow navigation must not write settings");
        }
    }
}

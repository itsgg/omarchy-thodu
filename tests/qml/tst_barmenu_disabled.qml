import QtQuick
import QtTest
import "../../components" as Components
import "../../lib/BarSettings.js" as BarSettings
import "../../lib/BarMenuActions.js" as MenuActions

// REQ-SHELL-02: with a configuration that is invalid or could not be read,
// the menu's disabled rows do nothing from the keyboard either, and opening
// the editor never writes over a file that was not read.
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
        settingsReady: false
        configEditable: false
    }

    SignalSpy {
        id: actions
        target: menu
        signalName: "actionRequested"
    }

    TestCase {
        name: "ThoduBarMenuDisabled"
        when: windowShown

        function init() {
            menu.settingsReady = false;
            menu.configEditable = false;
            menu.notice = "";
            menu.open = true;
            menu.selectedIndex = 0;
            actions.clear();
        }

        function test_req_shell_02_enter_on_a_disabled_row_does_nothing() {
            for (let index = 3; index <= 9; index++) {
                menu.open = true;
                menu.selectedIndex = index;
                menu.activateSelected();
                menu.moveSelection(1, 0);
                menu.selectedIndex = index;
                menu.moveSelection(-1, 0);
                compare(actions.count, 0, "row " + index + " is disabled and must not act");
            }
            menu.choose("status_display", "off");
            compare(actions.count, 0, "a choice is disabled too");
            verify(menu.open, "a disabled row does not close the menu");
        }

        function test_req_shell_02_modes_and_the_editor_stay_available() {
            menu.selectedIndex = 0;
            menu.activateSelected();
            compare(actions.count, 1, "a mode needs no configuration");
            compare(actions.signalArguments[0][0].kind, "launch");
            menu.open = true;
            menu.configEditable = true;
            menu.selectedIndex = 9;
            menu.activateSelected();
            compare(actions.count, 2, "an invalid but readable file can be edited");
            compare(actions.signalArguments[1][0].kind, "edit-config");
        }

        function test_req_shell_02_steppers_at_a_bound_do_not_act() {
            menu.settingsReady = true;
            menu.configEditable = true;
            menu.options = { labels_only: true, status_display: "auto", scroll_step: 1, scroll_shift_step: 100 };
            menu.selectedIndex = 5;
            menu.activateSelected();
            menu.selectedIndex = 8;
            menu.activateSelected();
            compare(actions.count, 0, "a step at its bound is disabled");
            menu.selectedIndex = 6;
            menu.activateSelected();
            compare(actions.count, 1);
            compare(actions.signalArguments[0][0].value, 2);
        }

        function test_req_shell_02_enabled_rows_follow_the_configuration() {
            const options = { scroll_step: 50, scroll_shift_step: 50 };
            for (let i = 0; i <= 2; i++)
                verify(MenuActions.enabledFor(i, options, false, false));
            for (let i = 3; i <= 8; i++) {
                verify(!MenuActions.enabledFor(i, options, false, true), "row " + i);
                verify(MenuActions.enabledFor(i, options, true, true), "row " + i);
            }
            verify(!MenuActions.enabledFor(9, options, true, false));
            verify(MenuActions.enabledFor(9, options, false, true));
            verify(!MenuActions.enabledFor(10, options, true, true));
        }

        function test_req_shell_02_the_editor_never_writes_over_an_unread_file() {
            compare(BarSettings.editorStart({ configReady: false, configDirectoryReady: true, configText: "" }), null,
                    "an unreadable file is neither written nor opened");
            compare(BarSettings.editorStart({ configReady: true, configDirectoryReady: false, configText: "" }), null);
            compare(BarSettings.editorStart({ configReady: true, configDirectoryReady: true, configText: "" }),
                    BarSettings.defaultsText(), "a missing file starts from the defaults");
            compare(BarSettings.editorStart({ configReady: true, configDirectoryReady: true, configText: "scroll_step = 0\n" }), "",
                    "a file with text opens as it is");
        }

        function test_req_shell_02_a_failed_save_is_shown() {
            menu.notice = "Could not save config.toml; edit it to fix";
            const hint = findChild(menu, "thoduBarMenuHint");
            verify(hint !== null, "the footer hint exists");
            compare(hint.text, menu.notice);
        }
    }
}

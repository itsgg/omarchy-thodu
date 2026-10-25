import QtQuick
import QtTest
import "../../lib/BarSettings.js" as BarSettings
import "../../lib/BarMenuActions.js" as MenuActions

// Verify menu actions and persisted settings.
TestCase {
    name: "ThoduBarSettings"

    function test_req_shell_02_settings_parse_with_defaults_and_bounds() {
        compare(BarSettings.parse(BarSettings.defaultsText()), BarSettings.defaults());
        const parsed = BarSettings.parse("labels_only = true\nstatus_display = \"always\" # keep this comment\nscroll_step = 120\nscroll_shift_step = 0\n");
        compare(parsed.labels_only, true);
        compare(parsed.status_display, "always");
        compare(parsed.scroll_step, 1);
        compare(parsed.scroll_shift_step, 5);
    }

    function test_req_shell_02_settings_write_preserves_other_values_and_comments() {
        const original = "# user note\nlabels_only = true\nstatus_display = \"auto\" # status comment\nscroll_shift_step = 2\n";
        const updated = BarSettings.setValue(original, "scroll_step", 9);
        verify(updated.indexOf("# user note") >= 0);
        verify(updated.indexOf("labels_only = true") >= 0);
        verify(updated.indexOf("status_display = \"auto\" # status comment") >= 0);
        verify(updated.indexOf("scroll_shift_step = 2") >= 0);
        compare(BarSettings.parse(updated).scroll_step, 9);
        verify(BarSettings.isSupported(updated));
    }

    function test_req_shell_02_setting_updates_keep_a_single_final_newline() {
        const once = BarSettings.setValue("scroll_step = 1\n", "scroll_step", 2);
        const twice = BarSettings.setValue(once, "scroll_step", 3);
        compare(once, "scroll_step = 2\n");
        compare(twice, "scroll_step = 3\n");
    }

    function test_req_shell_02_settings_reject_toml_leading_zero_integers() {
        verify(!BarSettings.isSupported("scroll_step = 05\n"));
        verify(!BarSettings.isSupported("scroll_step = 0_5\n"));
        verify(BarSettings.isSupported("scroll_step = 5\n"));
        verify(BarSettings.isSupported("scroll_step = 1_0\n"));
    }

    function test_req_shell_02_settings_write_only_changes_top_level_keys() {
        const original = "[other]\nscroll_step = 40\n";
        const updated = BarSettings.setValue(original, "scroll_step", 3);
        verify(updated.indexOf("scroll_step = 40") >= 0);
        verify(updated.indexOf("scroll_step = 3\n[other]") >= 0);
        verify(!updated.endsWith("\n\n"));
        compare(BarSettings.parse(updated).scroll_step, 3);
        verify(!BarSettings.isSupported(updated));
    }

    function test_req_shell_02_settings_parse_toml_digit_separators() {
        compare(BarSettings.parse("scroll_step = 1_0\n").scroll_step, 10);
        verify(BarSettings.isSupported("scroll_step = 1_0\n"));
    }

    function test_req_shell_02_session_buttons_dispatch_and_close_the_menu() {
        const settings = BarSettings.defaults();
        const hints = MenuActions.actionFor(0, settings, 0);
        const grid = MenuActions.actionFor(1, settings, 0);
        const scroll = MenuActions.actionFor(2, settings, 0);
        compare(hints, { kind: "launch", mode: "hints" });
        compare(grid, { kind: "launch", mode: "grid" });
        compare(scroll, { kind: "launch", mode: "scroll" });
        verify(MenuActions.closesAfter(hints));
        verify(MenuActions.closesAfter(grid));
        verify(MenuActions.closesAfter(scroll));
        verify(!MenuActions.closesAfter(MenuActions.actionFor(3, settings, 0)));
    }

    function test_req_shell_02_setting_actions_toggle_and_bound_scroll_steps() {
        const settings = { labels_only: false, status_display: "auto", scroll_step: 100, scroll_shift_step: 1 };
        compare(MenuActions.actionFor(3, settings, 0).value, true);
        compare(MenuActions.actionFor(4, settings, 1).value, "always");
        compare(MenuActions.actionFor(4, settings, -1).value, "off");
        compare(MenuActions.actionFor(6, settings, 0).value, 100);
        compare(MenuActions.actionFor(6, settings, -1).value, 99);
        compare(MenuActions.actionFor(7, settings, 0).value, 1);
    }
}

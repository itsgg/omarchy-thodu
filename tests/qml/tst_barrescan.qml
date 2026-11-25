import QtQuick
import QtTest
import "../../components"

// REQ-SHELL-01: a Hyprland event that may move the bar's items asks for a
// scan at once and again once the bar has settled; other events ask for none.
TestCase {
    name: "BarRescan"
    when: windowShown

    QtObject {
        id: hyprland
        signal rawEvent(var event)
    }

    BarRescan {
        id: rescan
        events: hyprland
        settleMs: 300
    }

    SignalSpy {
        id: due
        target: rescan
        signalName: "due"
    }

    function init() {
        wait(400);
        due.clear();
    }

    function test_req_shell_01_a_workspace_event_rescans_now_and_after_settling() {
        hyprland.rawEvent({ name: "workspacev2", data: "2,2" });
        tryVerify(() => due.count === 1, 250, "a scan before the bar has settled");
        tryVerify(() => due.count === 2, 2000, "a second scan once the bar has settled");
        wait(400);
        compare(due.count, 2);
    }

    function test_req_shell_01_a_burst_of_window_events_coalesces() {
        hyprland.rawEvent({ name: "openwindow", data: "" });
        hyprland.rawEvent({ name: "activewindowv2", data: "" });
        hyprland.rawEvent({ name: "movewindowv2", data: "" });
        tryVerify(() => due.count === 2, 2000);
        wait(400);
        compare(due.count, 2, "one scan now and one after settling, not one per event");
    }

    function test_req_shell_01_an_event_that_moves_no_bar_item_asks_for_no_scan() {
        hyprland.rawEvent({ name: "submap", data: "resize" });
        wait(400);
        compare(due.count, 0);
    }
}

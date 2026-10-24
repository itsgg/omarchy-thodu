import QtQuick
import QtTest
import "../../components"

TestCase {
    name: "WorkspaceGuard"
    when: windowShown

    QtObject {
        id: monitor
        property var activeWorkspace: null
    }

    QtObject {
        id: monitorWithoutActiveWorkspace
    }

    WorkspaceGuard {
        id: guard
    }

    SignalSpy {
        id: deactivated
        target: guard
        signalName: "deactivated"
    }

    function init() {
        guard.watching = false;
        monitor.activeWorkspace = ({ id: 1 });
        deactivated.clear();
        guard.bind(monitor, 1);
    }

    function test_req_session_01_monitor_workspace_change_ends_the_guard() {
        verify(guard.workspaceMatches);
        monitor.activeWorkspace = ({ id: 2 });
        tryVerify(() => deactivated.count === 1, 1000,
                  "a new active workspace deactivates the old session");
        verify(!guard.watching);
    }

    function test_req_session_01_removed_workspace_ends_the_guard() {
        monitor.activeWorkspace = null;
        tryVerify(() => deactivated.count === 1, 1000,
                  "removing the active workspace deactivates the session");
        verify(!guard.watching);
    }

    function test_req_session_01_undefined_active_workspace_ends_the_guard() {
        guard.watching = false;
        deactivated.clear();
        guard.bind(monitorWithoutActiveWorkspace, 1);
        compare(deactivated.count, 1,
                "an undefined activeWorkspace deactivates instead of throwing");
        verify(!guard.watching);
    }

    function test_req_session_01_late_binding_uses_the_start_workspace() {
        guard.watching = false;
        monitor.activeWorkspace = ({ id: 2 });
        deactivated.clear();
        guard.bind(monitor, 1);
        compare(deactivated.count, 1,
                "a reply arriving after a workspace switch cannot bind to the new workspace");
        verify(!guard.watching);
    }
}

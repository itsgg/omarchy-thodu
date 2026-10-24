import QtQuick

// End a monitor-wide session when its starting workspace is no longer active.
// Watching the monitor's activeWorkspace also covers workspaces that vanish
// from Hyprland's workspace model as soon as they are left.
Item {
    id: guard

    visible: false
    width: 0
    height: 0

    property var monitor: null
    property var workspaceId: null
    property bool watching: false
    readonly property bool workspaceMatches:
        guard.monitor !== null && guard.monitor.activeWorkspace != null &&
        guard.workspaceId !== null && guard.monitor.activeWorkspace.id === guard.workspaceId

    onWorkspaceMatchesChanged: guard.check()

    signal deactivated

    function bind(activeMonitor, startingWorkspaceId) {
        guard.watching = false;
        guard.monitor = activeMonitor;
        guard.workspaceId = startingWorkspaceId;
        guard.watching = true;
        guard.check();
    }

    function check() {
        if (!guard.watching || guard.workspaceMatches)
            return;
        guard.watching = false;
        guard.deactivated();
    }

}

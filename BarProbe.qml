import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "components"
import "lib/BarScan.js" as BarScan
import "lib/BarSettings.js" as BarSettings

// Visible bar control and bar target reporter (REQ-SHELL-01, REQ-SHELL-02).
BarWidget {
    id: probe
    moduleName: "io.github.itsgg.thodu"

    property var shell: null
    property var manifest: null
    property string last: ""
    property string serviceState: "checking"
    property string configText: ""
    property bool configDirectoryReady: false
    property bool configReady: false
    property bool settingsValid: true
    property var options: BarSettings.defaults()

    readonly property string socketPath: Quickshell.env("XDG_RUNTIME_DIR") + "/thodu.sock"
    readonly property string configDirectory: (Quickshell.env("XDG_CONFIG_HOME") || (Quickshell.env("HOME") + "/.config")) + "/thodu"
    readonly property string configPath: configDirectory + "/config.toml"
    readonly property bool serviceReady: serviceState === "active"
    readonly property bool opened: menu.open

    implicitWidth: button.implicitWidth
    implicitHeight: button.implicitHeight

    function report() {
        const items = BarScan.scan(BarScan.rootOf(probe), probe);
        const request = { op: "shell-items", screen: probe.Screen.name, items: items };
        const line = JSON.stringify(request);
        if (line === probe.last && client.connected)
            return;
        probe.last = line;
        client.connectTo(probe.socketPath);
        client.send(request);
    }

    function open() {
        menu.open = true;
    }

    function close() {
        menu.open = false;
    }

    function toggle() {
        if (probe.opened) probe.close();
        else probe.open();
    }

    function closeForPopoutSwitch() {
        probe.close();
    }

    function refreshService() {
        if (!serviceProbe.running)
            serviceProbe.running = true;
    }

    function launchMode(mode) {
        const host = probe.bar;
        if (!host || typeof host.run !== "function")
            return;
        const start = probe.serviceReady ? "" : "systemctl --user start thodud.service && ";
        probe.close();
        Qt.callLater(function() { host.run(start + "thodu " + mode); });
    }

    function writeSetting(key, value) {
        if (!probe.configReady || !probe.configDirectoryReady || !probe.settingsValid)
            return;
        const next = BarSettings.setValue(probe.configText, key, value);
        if (next === probe.configText)
            return;
        probe.configText = next;
        probe.options = BarSettings.parse(next);
        configFile.setText(next);
        probe.report();
    }

    function openConfig() {
        const host = probe.bar;
        if (!host || typeof host.run !== "function")
            return;
        if (probe.configText.trim() === "") {
            const initial = BarSettings.defaultsText();
            probe.configText = initial;
            probe.options = BarSettings.parse(initial);
            configFile.setText(initial);
        }
        probe.close();
        Qt.callLater(function() {
            host.run("omarchy-launch-floating-terminal-with-presentation nvim " + Util.shellQuote(probe.configPath));
        });
    }

    function handleMenuAction(action) {
        if (!action || typeof action !== "object")
            return;
        if (action.kind === "launch") {
            probe.launchMode(String(action.mode || "hints"));
        } else if (action.kind === "setting") {
            probe.writeSetting(String(action.key || ""), action.value);
        } else if (action.kind === "edit-config") {
            probe.openConfig();
        }
    }

    WidgetButton {
        id: button
        anchors.fill: parent
        bar: probe.bar
        text: "T"
        fontSize: Style.font.caption
        horizontalMargin: 7
        verticalPadding: 5
        active: probe.opened
        activeColor: probe.serviceReady ? Commons.Color.accent : Commons.Color.urgent
        tooltipText: probe.serviceReady
            ? "Thodu ready - click for modes and settings"
            : "Thodu service stopped - click to start a mode or settings"
        onPressed: probe.toggle()

        Rectangle {
            width: Style.space(6)
            height: width
            radius: width / 2
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.rightMargin: Style.space(2)
            anchors.topMargin: Style.space(2)
            color: probe.serviceReady ? Commons.Color.accent : Commons.Color.urgent
            border.width: Style.space(1)
            border.color: Commons.Color.background
        }
    }

    BarMenu {
        id: menu
        bar: probe.bar
        anchorItem: button
        owner: probe
        serviceState: probe.serviceState
        options: probe.options
        settingsReady: probe.configReady && probe.settingsValid
        configEditable: probe.configReady
        configPath: probe.configPath
        onActionRequested: function(action) { probe.handleMenuAction(action) }
    }

    Client {
        id: client
        keepLatest: true
    }

    Process {
        id: serviceProbe
        command: ["systemctl", "--user", "is-active", "thodud.service"]
        stdout: StdioCollector {
            waitForEnd: true
            onStreamFinished: probe.serviceState = text.trim() || "inactive"
        }
        onExited: function(exitCode) {
            if (exitCode !== 0 && probe.serviceState === "active")
                probe.serviceState = "inactive"
        }
    }

    Timer {
        interval: 5000
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: probe.refreshService()
    }

    Process {
        id: ensureConfigDirectory
        command: ["install", "-d", "-m", "700", probe.configDirectory]
        onExited: function(exitCode) {
            probe.configDirectoryReady = exitCode === 0;
            if (probe.configDirectoryReady)
                configFile.reload();
        }
    }

    FileView {
        id: configFile
        path: probe.configPath
        watchChanges: true
        atomicWrites: true
        printErrors: false
        onLoaded: {
            probe.configText = text();
            probe.options = BarSettings.parse(probe.configText);
            probe.settingsValid = BarSettings.isSupported(probe.configText);
            probe.configReady = probe.configDirectoryReady;
        }
        onLoadFailed: function(error) {
            if (error === FileViewError.FileNotFound) {
                probe.configText = "";
                probe.options = BarSettings.defaults();
                probe.settingsValid = true;
                probe.configReady = probe.configDirectoryReady;
            } else {
                probe.configReady = false;
                console.warn("thodu: cannot read " + probe.configPath);
            }
        }
        onFileChanged: configFile.reload()
    }

    Timer {
        interval: 1000
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: probe.report()
    }

    Component.onCompleted: {
        ensureConfigDirectory.running = true;
    }
}

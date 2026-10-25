pragma Singleton
import QtQuick

// The tokens the plugin reads, at Omarchy's defaults.
QtObject {
    readonly property QtObject font: QtObject {
        property string family: "monospace"
        property int title: 16
        property int subtitle: 13
        property int body: 14
        property int bodySmall: 11
        property int caption: 11
    }

    readonly property QtObject spacing: QtObject {
        property int xs: 3
        property int huge: 18
        property int rowPaddingX: 12
        property int controlHeight: 28
        property int controlPaddingY: 6
    }

    readonly property real cornerRadius: 0

    function space(value) { return value }
    function controlFill(focused, hot, foreground, accent) { return hot ? "#202020" : "transparent" }
}

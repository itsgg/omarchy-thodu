pragma Singleton
import QtQuick

QtObject {
    readonly property QtObject font: QtObject {
        property string family: "monospace"
        property int body: 14
        property int caption: 11
    }

    function space(value) { return value }
}

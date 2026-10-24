import QtQuick

QtObject {
    property bool opened: false
    property bool testing: false
    property bool panelVisible: false
    property bool pointerPassThrough: false
    property bool lingering: false
    property bool focusReleasePending: false
    property int keyboardFocusNone: 0
    property int keyboardFocusMode: 0

    readonly property bool grabActive: opened && !testing && panelVisible
                                      && !pointerPassThrough && !lingering
                                      && !focusReleasePending
    readonly property int layerKeyboardFocus: !opened || testing
                                               ? keyboardFocusNone
                                               : keyboardFocusMode
}

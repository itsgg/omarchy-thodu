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

    // After a lingering click (REQ-ACT-02) the overlay lets the pointer
    // through with on-demand keyboard focus, and takes the keyboard back as
    // exclusive, for a second Return, only on the hidden reply that says the
    // current session's first click is done without a workspace switch,
    // while the wait still runs. A reply that comes after the wait ended
    // must not undo the on-demand frame the overlay closes with, and one
    // that says clicked false leaves focus on demand until the wait ends.
    function reclaimsAfterClick(reply, clickSession, viewSession, lingering) {
        return reply !== null && reply !== undefined && reply.clicked === true
            && reply.workspace_changed !== true && clickSession !== ""
            && reply.session === clickSession && clickSession === viewSession
            && lingering === true;
    }
}

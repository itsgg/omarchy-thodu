import QtQuick

Item {
    signal moveRequested(int dx, int dy)
    signal activateRequested()
    signal returnRequested()
    signal closeRequested()
    signal deleteRequested()
    signal tabRequested(int direction)
    signal textKey(string text)
}

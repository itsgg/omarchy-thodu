import QtQuick
import QtTest

TestCase {
    name: "Smoke"

    Loader {
        id: loader
        source: "../../Thodu.qml"
    }

    function test_toggle_opens_and_closes() {
        verify(loader.item !== null);
        loader.item.toggle();
        compare(loader.item.opened, true);
        loader.item.toggle();
        compare(loader.item.opened, false);
    }
}

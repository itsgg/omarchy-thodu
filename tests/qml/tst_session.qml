import QtQuick
import QtTest
import "../../components"

// The overlay's session view against a fake IPC: Escape gives the keyboard
// back (REQ-SESSION-02), silence closes it within a second
// (REQ-SESSION-03), keys reach thodud as SPEC section 7 names them, an
// action hands over to thodud, and labels sit where thodud placed them.
Item {
    id: top
    width: 800
    height: 600

    property var sent: []
    property bool answering: true
    property var ended: null
    property real lastReplyAt: 0
    // The state the fake reports for the next key, and the session's view.
    property string nextState: "showing"
    property var shownView: ({ state: "showing", visible: ["a0", "a1"], focused: null, search: "", label_prefix: "" })

    readonly property var fullReply: ({
            v: 1,
            ok: true,
            session: "s1",
            monitor: { name: "TEST", area: { x: 100, y: 50, w: 800, h: 600 }, scale: 1 },
            targets: [
                { id: "a0", bounds: { x: 120, y: 80, w: 60, h: 30 }, role: "button", text: "Open", source: "atspi", label: "a", placed: { x: 113, y: 86, w: 17, h: 18 } },
                { id: "a1", bounds: { x: 220, y: 80, w: 60, h: 30 }, role: "button", text: "Save", source: "atspi", label: "s", placed: { x: 213, y: 86, w: 17, h: 18 } }
            ],
            view: { state: "showing", visible: ["a0", "a1"], focused: null, search: "", label_prefix: "" },
            status: "2 targets from atspi in 3 ms"
        })

    function answer(request) {
        if (!top.answering)
            return;
        let reply;
        if (request.op === "session" && !request.brief) {
            reply = top.fullReply;
        } else if (request.op === "session") {
            reply = { v: 1, ok: true, session: "s1", view: top.shownView };
        } else if (request.op === "key") {
            const state = request.key === "Escape" ? "closed" : top.nextState;
            top.shownView = Object.assign({}, top.shownView, { state: state });
            reply = { v: 1, ok: true, session: "s1", view: top.shownView };
        } else {
            reply = { v: 1, ok: true };
        }
        Qt.callLater(() => {
            top.lastReplyAt = Date.now();
            view.apply(reply);
        });
    }

    function keysSent() {
        return top.sent.filter(r => r.op === "key");
    }

    SessionView {
        id: view
        anchors.fill: parent
        heartbeatMs: 50
        onSend: request => {
            top.sent.push(request);
            top.answer(request);
        }
        onFinished: (why, acting) => top.ended = { why: why, acting: acting }
    }

    TestCase {
        name: "Session"
        when: windowShown

        function init() {
            top.sent = [];
            top.answering = true;
            top.ended = null;
            top.nextState = "showing";
            top.shownView = { state: "showing", visible: ["a0", "a1"], focused: null, search: "", label_prefix: "" };
            view.begin("s1");
            view.forceActiveFocus();
            tryVerify(() => view.loaded, 1000, "the session loaded");
        }

        function cleanup() {
            view.end("test over", false);
        }

        function test_req_session_02_escape_ends_the_session() {
            verify(view.activeFocus, "the view has the keyboard while showing");
            keyClick(Qt.Key_Escape);
            tryVerify(() => top.ended !== null, 1000, "the session ended");
            compare(top.keysSent().length, 1);
            compare(top.keysSent()[0].key, "Escape");
            verify(!view.active, "the view no longer takes keys");
            verify(!view.activeFocus, "the view gave the keyboard back");
        }

        // Escape gives the keyboard back at once, before and without any
        // answer from thodud, and an ended view sends nothing even with the
        // focus forced back onto it.
        function test_req_session_02_escape_needs_no_answer() {
            top.answering = false;
            keyClick(Qt.Key_Escape);
            const keys = top.keysSent();
            compare(keys.length, 1);
            compare(keys[0].key, "Escape");
            verify(top.ended !== null, "the session ended");
            compare(top.ended.acting, false);
            verify(!view.active, "the view no longer takes keys");
            verify(!view.activeFocus, "the view gave the keyboard back");
            view.forceActiveFocus();
            keyClick(Qt.Key_A);
            compare(top.keysSent().length, 1, "no key after Escape reaches thodud");
        }

        function test_req_session_03_silence_closes_within_a_second() {
            top.answering = false;
            const stopped = Math.max(top.lastReplyAt, Date.now() - 60);
            tryVerify(() => top.ended !== null, 1000, "the view closed on silence");
            verify(Date.now() - stopped <= 1000, "closed " + (Date.now() - stopped) + " ms after the last reply");
            compare(top.ended.why, "thodud stopped answering");
            verify(!view.active);
        }

        function test_heartbeats_keep_a_session_open() {
            wait(view.silenceMs + 200);
            verify(view.active, "a live thodud keeps the session");
            verify(top.sent.some(r => r.op === "session" && r.brief === true), "heartbeats are brief");
        }

        function test_keys_reach_thodud_as_the_spec_names_them() {
            keyClick(Qt.Key_S, Qt.ShiftModifier);
            keyClick(Qt.Key_L);
            keyClick(Qt.Key_Tab);
            keyClick(Qt.Key_Backtab, Qt.ShiftModifier);
            keyClick(Qt.Key_Backspace);
            keyClick(Qt.Key_Return, Qt.ControlModifier);
            const keys = top.keysSent().map(k => k.key + (k.shift ? "+shift" : "") + (k.ctrl ? "+ctrl" : ""));
            compare(keys, ["s+shift", "l", "Tab", "Backtab+shift", "BackSpace", "Return+ctrl"]);
            verify(top.keysSent().every(k => k.session === "s1"));
        }

        function test_acting_hands_over_to_thodud() {
            top.nextState = "acting";
            keyClick(Qt.Key_Return);
            tryVerify(() => top.ended !== null, 1000);
            compare(top.ended.acting, true, "the overlay must hide and say so");
            verify(!view.active);
        }

        function test_an_error_reply_ends_the_session() {
            view.apply({ v: 1, ok: false, error: "bad-request", message: "no session s1" });
            verify(top.ended !== null);
            compare(top.ended.why, "no session s1");
        }

        function test_labels_sit_where_thodud_placed_them() {
            const chip = findChild(view, "label-s");
            verify(chip !== null);
            compare(chip.x, 213 - 100);
            compare(chip.y, 86 - 50);
            compare(chip.width, 17);
            verify(chip.visible);
            top.shownView = { state: "showing", visible: ["a0"], focused: "a0", search: "o", label_prefix: "" };
            tryVerify(() => !findChild(view, "label-s").visible, 1000, "a label the search drops is hidden");
            verify(findChild(view, "label-a").visible);
        }

        // A session thodud pushed draws from its reply, with no request.
        function test_a_pushed_session_draws_at_once() {
            view.end("test over", false);
            top.sent = [];
            top.ended = null;
            const pushed = Object.assign({}, top.fullReply, { session: "s2" });
            view.begin("s2", pushed);
            verify(view.loaded, "drawn from the pushed reply");
            compare(view.session, "s2");
            compare(top.sent.filter(r => r.op === "session" && !r.brief).length, 0);
            verify(findChild(view, "label-a") !== null);
        }

        function test_replies_for_another_session_are_ignored() {
            view.apply({ v: 1, ok: false, session: "s0", message: "no session s0" });
            verify(view.active);
            verify(top.ended === null);
        }
    }
}

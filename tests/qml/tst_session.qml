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
    property bool nextLinger: false
    // Targets the fake sends with the next key reply, as the grid does.
    property var nextTargets: null
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
            const generation = top.nextTargets ? (top.shownView.generation || 0) + 1 : (top.shownView.generation || 0);
            top.shownView = Object.assign({}, top.shownView, { state: state, linger: state === "acting" && top.nextLinger, generation: generation });
            reply = { v: 1, ok: true, session: "s1", view: top.shownView };
            if (top.nextTargets) {
                reply.targets = top.nextTargets;
                top.nextTargets = null;
            }
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
            top.nextLinger = false;
            top.nextTargets = null;
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
            keyClick(Qt.Key_Slash);
            compare(keys.concat(["/"]).length, top.keysSent().length);
            compare(keys, ["S+shift", "l", "Tab", "Backtab+shift", "BackSpace", "Return+ctrl"]);
            compare(top.keysSent()[6].key, "/", "REQ-SEARCH-02: slash starts a search in labels-only mode");
            verify(top.keysSent().every(k => k.session === "s1"));
        }

        function test_req_search_01_uppercase_key_text_is_preserved() {
            const request = view.keyRequest(Qt.Key_A, Qt.NoModifier, "A");
            compare(request.key, "A");
            compare(request.shift, false);
        }

        function test_req_search_01_shifted_letter_key_event_is_uppercase() {
            keyClick(Qt.Key_D, Qt.ShiftModifier);
            compare(top.keysSent().length, 1);
            compare(top.keysSent()[0].key, "D");
            verify(top.keysSent()[0].shift);
        }

        function test_req_act_02_return_auto_repeat_is_ignored() {
            compare(view.keyRequest(Qt.Key_Return, Qt.NoModifier, "", true), null);
            compare(view.keyRequest(Qt.Key_Return, Qt.NoModifier, "", false).key, "Return");
        }

        function test_acting_hands_over_to_thodud() {
            top.nextState = "acting";
            keyClick(Qt.Key_Return);
            tryVerify(() => top.ended !== null, 1000);
            compare(top.ended.acting, true, "the overlay must hide and say so");
            verify(!view.active);
        }

        // REQ-ACT-02: after a lingering click the view keeps the keyboard,
        // draws nothing, and a second Return goes to thodud.
        function test_req_act_02_a_click_keeps_the_keyboard_for_a_second_return() {
            let lingered = 0;
            const count = () => lingered++;
            view.lingered.connect(count);
            top.nextState = "acting";
            top.nextLinger = true;
            keyClick(Qt.Key_Return);
            tryVerify(() => view.lingering, 1000, "the view waits");
            compare(lingered, 1, "the overlay is told to let the pointer through");
            verify(view.active && view.activeFocus, "it keeps the keyboard");
            verify(!findChild(view, "label-a").visible, "nothing is drawn");
            top.nextState = "closed";
            keyClick(Qt.Key_Return);
            compare(top.keysSent().map(k => k.key), ["Return", "Return"]);
            tryVerify(() => top.ended !== null, 1000);
            compare(top.ended.acting, false);
            verify(!view.activeFocus);
            view.lingered.disconnect(count);
        }

        function test_req_act_02_the_wait_ends_by_itself() {
            top.nextState = "acting";
            top.nextLinger = true;
            keyClick(Qt.Key_Return);
            tryVerify(() => view.lingering, 1000);
            top.answering = false;
            tryVerify(() => top.ended !== null, view.lingerMs + 200, "the wait ends");
            compare(top.ended.why, "no second Return");
        }

        function test_req_act_02_workspace_change_ends_the_linger() {
            top.nextState = "acting";
            top.nextLinger = true;
            keyClick(Qt.Key_Return);
            tryVerify(() => view.lingering, 1000, "the view waits after the click");
            view.apply({ v: 1, ok: true, session: "s1", workspace_changed: true });
            verify(top.ended !== null, "a workspace switch ends the old session");
            compare(top.ended.why, "workspace changed");
            compare(top.ended.acting, false);
            verify(!view.activeFocus, "the overlay gives the keyboard back");
        }

        function test_req_act_02_another_key_ends_the_wait_at_once() {
            top.nextState = "acting";
            top.nextLinger = true;
            keyClick(Qt.Key_Return);
            tryVerify(() => view.lingering, 1000);
            top.answering = false;
            keyClick(Qt.Key_Return, Qt.ShiftModifier);
            verify(top.ended !== null, "a modified Return is not a double click");
            verify(!view.activeFocus);
        }

        function test_req_act_02_a_click_without_the_wait_hands_over() {
            top.nextState = "acting";
            top.nextLinger = false;
            keyClick(Qt.Key_Return, Qt.ShiftModifier);
            tryVerify(() => top.ended !== null, 1000);
            compare(top.ended.acting, true);
            verify(!view.lingering);
        }

        // REQ-GRID-01: Space opens the grid, whose cells replace the targets.
        function test_req_grid_01_cells_replace_the_targets() {
            top.nextTargets = [
                { id: "g0", bounds: { x: 100, y: 50, w: 400, h: 600 }, role: "unknown", text: "", source: "grid", label: "a", placed: { x: 100, y: 50, w: 17, h: 18 } },
                { id: "g1", bounds: { x: 500, y: 50, w: 400, h: 600 }, role: "unknown", text: "", source: "grid", label: "s", placed: { x: 500, y: 50, w: 17, h: 18 } }
            ];
            top.shownView = Object.assign({}, top.shownView, { visible: ["g0", "g1"] });
            keyClick(Qt.Key_Space);
            compare(top.keysSent()[0].key, "space");
            tryVerify(() => view.targets.length === 2 && view.targets[0].id === "g0", 1000, "the cells are drawn");
            compare(view.generation, 1);
            verify(findChild(view, "label-s").visible);
        }

        // REQ-GRID-01: a cell's own cells are drawn magnified beside it,
        // with readable labels, and their tiny labels are not drawn.
        function test_req_grid_01_small_cells_are_magnified() {
            const cell = (i, label) => ({ id: "g" + i, bounds: { x: 300 + (i % 6) * 10, y: 200 + Math.floor(i / 6) * 8, w: 10, h: 8 }, role: "unknown", text: "", source: "grid", label: label, placed: { x: 295 + (i % 6) * 10, y: 196 + Math.floor(i / 6) * 8, w: 17, h: 18 } });
            const letters = "asdfghjklqwertyuiopzxcvb";
            top.nextTargets = Array.from(letters).map((l, i) => cell(i, l));
            top.shownView = Object.assign({}, top.shownView, { visible: top.nextTargets.map(t => t.id) });
            keyClick(Qt.Key_Space);
            tryVerify(() => view.zoomed, 1000, "a sub-level is magnified");
            const zoom = findChild(view, "zoom");
            verify(zoom.visible);
            verify(zoom.factor >= 5, "magnified at least five times, not " + zoom.factor);
            const big = findChild(view, "zoom-d");
            verify(big !== null && big.visible);
            compare(big.width, 10 * zoom.factor);
            verify(!findChild(view, "label-d").visible, "the tiny label is not drawn");
            verify(zoom.y >= view.gridArea.y - view.area.y + view.gridArea.h, "below the area, not over it");
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

        function test_req_session_01_label_focus_uses_chip_and_status_wraps() {
            const longStatus = "atspi missed its 80 ms budget; 22 targets from the bar; " +
                "additional source details that should wrap inside the compact status toast";
            view.apply({
                v: 1, ok: true, session: "s1", status: longStatus,
                view: { state: "showing", visible: ["a0"], focused: "a0", search: "", label_prefix: "", label_selected: true }
            });
            const ring = findChild(view, "focus-ring");
            const chip = findChild(view, "label-a");
            const toast = findChild(view, "status-line");
            const message = findChild(view, "status-text");
            const cue = findChild(view, "hint-text");
            verify(ring !== null);
            verify(!ring.visible, "a label-focused target uses its chip without the large outline");
            verify(chip.visible);
            compare(chip.color, view.selectedBackground);
            verify(toast !== null);
            verify(message !== null && message.implicitHeight > 18, "long status wraps into more than one line");
            verify(cue.text.indexOf("uppercase final activates") !== -1);
            verify(toast.width <= 640, "the status toast stays within its maximum width");
            verify(toast.height > 38, "the wrapped status and activation cue fit in the toast");
            view.apply({
                v: 1, ok: true, session: "s1", status: longStatus,
                view: { state: "showing", visible: ["a0"], focused: "a0", search: "o", label_prefix: "", label_selected: false }
            });
            verify(ring.visible, "text-search focus keeps the target outline");
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

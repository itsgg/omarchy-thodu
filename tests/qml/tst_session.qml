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
    property var fallbackReply: null
    property bool sawScrollPassThrough: false
    property string scrollReplyStatus: ""
    property var shownView: ({ state: "showing", visible: ["a0", "a1"], focused: null, search: "", label_prefix: "", collecting: false })

    readonly property var fullReply: ({
            v: 1,
            ok: true,
            session: "s1",
            monitor: { name: "TEST", area: { x: 100, y: 50, w: 800, h: 600 }, scale: 1 },
            targets: [
                { id: "a0", bounds: { x: 120, y: 80, w: 60, h: 30 }, role: "button", text: "Open", source: "atspi", label: "a", placed: { x: 113, y: 86, w: 17, h: 18 } },
                { id: "a1", bounds: { x: 220, y: 80, w: 60, h: 30 }, role: "button", text: "Save", source: "atspi", label: "s", placed: { x: 213, y: 86, w: 17, h: 18 } }
            ],
            view: { state: "showing", visible: ["a0", "a1"], focused: null, search: "", label_prefix: "", collecting: false },
            status: "2 targets from atspi in 3 ms"
        })

    function answer(request) {
        if (!top.answering)
            return;
        let reply;
        if (request.op === "session" && !request.brief) {
            reply = top.fallbackReply || top.fullReply;
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
        } else if (request.op === "scroll-ready") {
            top.shownView = Object.assign({}, top.shownView, { state: "showing", scroll_mode: true });
            reply = { v: 1, ok: true, session: "s1", view: top.shownView,
                      status: top.scrollReplyStatus || top.fullReply.status };
            top.scrollReplyStatus = "";
        } else {
            reply = { v: 1, ok: true };
        }
        Qt.callLater(() => {
            top.lastReplyAt = Date.now();
            view.apply(reply);
            if (reply.view && reply.view.state === "acting" && reply.view.scroll_mode) {
                top.sawScrollPassThrough = view.scrollPassThrough;
                Qt.callLater(() => view.notifyScrollReady());
            }
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
            top.fallbackReply = null;
            top.sawScrollPassThrough = false;
            top.scrollReplyStatus = "";
            view.statusTimeoutMs = 2000;
            view.scrollRepeatMs = 80;
            top.shownView = { state: "showing", visible: ["a0", "a1"], focused: null, search: "", label_prefix: "", collecting: false };
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

        // REQ-SESSION-01: while the first labels are drawn, visual targets can
        // finish asynchronously without letting a pending label change under
        // a key. Escape stays available; keys resume after the new generation.
        function test_req_session_01_visual_collection_holds_keys_until_ready() {
            const pending = Object.assign({}, top.fullReply, {
                status: "collecting visual targets",
                view: Object.assign({}, top.fullReply.view, { collecting: true })
            });
            top.shownView = pending.view;
            view.apply(pending);
            verify(view.collecting);
            compare(view.hintCue(), "visual targets loading | Esc cancels");
            keyClick(Qt.Key_A);
            compare(top.keysSent().length, 0, "labels stay stable while targets load");

            const moreTargets = top.fullReply.targets.concat([
                { id: "i0", bounds: { x: 320, y: 80, w: 60, h: 30 }, role: "text", text: "", source: "image", actions: ["click"], window: "0x2", label: "d", placed: { x: 313, y: 86, w: 17, h: 18 } }
            ]);
            top.fallbackReply = Object.assign({}, top.fullReply, {
                targets: moreTargets,
                view: Object.assign({}, top.fullReply.view, { generation: 1, collecting: false })
            });
            top.shownView = Object.assign({}, top.fullReply.view, { generation: 1, collecting: false });
            top.answering = false;
            view.apply({ v: 1, ok: true, session: "s1", view: top.shownView });
            verify(view.fetching, "heartbeat detects the fallback generation and fetches its targets");
            verify(view.collecting, "old labels stay locked while the new labels are loading");
            keyClick(Qt.Key_A);
            compare(top.keysSent().length, 0, "a key cannot select a label from the old generation");

            top.answering = true;
            view.apply(top.fallbackReply);
            verify(!view.collecting);
            verify(!view.fetching);
            compare(view.targets.length, 3);
            keyClick(Qt.Key_A);
            compare(top.keysSent().length, 1, "keys resume after the fallback result arrives");
            compare(top.keysSent()[0].generation, 1, "key input names the generation shown to the user");
        }

        function test_req_session_01_drawn_event_reports_rendered_overlay_bounds() {
            view.area = { x: 100.5, y: 50.25, w: 800, h: 600 };
            const message = view.drawnMessage();
            const rects = message.overlays;
            const firstChip = findChild(view, "label-a");
            const secondChip = findChild(view, "label-s");
            const status = findChild(view, "status-line");
            verify(firstChip !== null && firstChip.visible);
            verify(secondChip !== null && secondChip.visible);
            verify(status !== null && status.visible);
            compare(message.op, "drawn");
            compare(message.session, "s1");
            compare(rects.length, 3, "two chips and the status toast are reported");
            compare(rects[0].x, Math.floor(view.area.x + firstChip.x));
            compare(rects[0].y, Math.floor(view.area.y + firstChip.y));
            compare(rects[0].w, Math.ceil(view.area.x + firstChip.x + firstChip.width) - rects[0].x);
            compare(rects[0].h, Math.ceil(view.area.y + firstChip.y + firstChip.height) - rects[0].y);
            compare(rects[1].x, Math.floor(view.area.x + secondChip.x));
            compare(rects[1].y, Math.floor(view.area.y + secondChip.y));
            const x = view.area.x + status.x;
            const y = view.area.y + status.y;
            compare(rects[2].x, Math.floor(x));
            compare(rects[2].y, Math.floor(y));
            compare(rects[2].w, Math.ceil(x + status.width) - Math.floor(x));
            compare(rects[2].h, Math.ceil(y + status.height) - Math.floor(y));

            view.focusedId = "a0";
            const focus = findChild(view, "focus-ring");
            verify(focus !== null && focus.visible);
            const focusedRects = view.overlayRects();
            compare(focusedRects.length, 4, "the focus ring is also reported");
            compare(focusedRects[2].x, Math.floor(view.area.x + focus.x));
            compare(focusedRects[2].y, Math.floor(view.area.y + focus.y));
            compare(focusedRects[2].w, Math.ceil(view.area.x + focus.x + focus.width) - focusedRects[2].x);
            compare(focusedRects[2].h, Math.ceil(view.area.y + focus.y + focus.height) - focusedRects[2].y);
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
            verify(top.keysSent().every(k => typeof k.generation === "number"),
                   "REQ-SESSION-01: every key request names its displayed generation");
        }

        // REQ-SESSION-07: numeric area selection is only sent in scroll mode;
        // REQ-ACT-06: the timer drives held repeats and OS repeats stay ignored.
        function test_req_session_07_scroll_keys_and_cue() {
            compare(view.keyRequest(Qt.Key_2, Qt.NoModifier, "2", false), null,
                    "digits stay ignored in hints mode");
            view.scrollMode = true;
            compare(view.keyRequest(Qt.Key_2, Qt.NoModifier, "2", false).key, "2");
            compare(view.keyRequest(Qt.Key_J, Qt.NoModifier, "j", false).key, "j");
            compare(view.keyRequest(Qt.Key_J, Qt.ShiftModifier, "J", false).key, "J");
            compare(view.keyRequest(Qt.Key_J, Qt.NoModifier, "j", true), null,
                    "auto-repeat cannot send multiple wheel actions");
            compare(view.hintCue(), "hold HJKL to scroll | Shift larger | Tab cycles | digits select | Esc exits");
            view.targets = [];
            compare(view.hintCue(), "no scroll areas | Esc exits");
        }

        function test_req_search_01_caps_lock_text_does_not_activate() {
            const request = view.keyRequest(Qt.Key_A, Qt.NoModifier, "A", false);
            compare(request.key, "a");
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

        function test_req_act_01_shifted_letter_auto_repeat_is_ignored() {
            compare(view.keyRequest(Qt.Key_A, Qt.ShiftModifier, "a", true), null);
            const request = view.keyRequest(Qt.Key_A, Qt.ShiftModifier, "a", false);
            compare(request.key, "A");
            verify(request.shift);
        }

        function test_req_search_01_label_repeats_are_ignored_but_search_repeats_are_kept() {
            view.labelsOnly = true;
            view.searching = false;
            compare(view.keyRequest(Qt.Key_A, Qt.NoModifier, "a", true), null,
                    "a repeated lowercase label key cannot complete a prefix");
            view.searching = true;
            compare(view.keyRequest(Qt.Key_A, Qt.NoModifier, "a", true).key, "a",
                    "search text keeps normal key repeat");
            view.labelPrefix = "a";
            compare(view.keyRequest(Qt.Key_A, Qt.NoModifier, "a", true), null,
                    "a lowercase repeat cannot complete an active prefix");
        }

        // REQ-SESSION-01: only labels matching the active prefix are styled
        // as a partial selection.
        function test_req_session_01_only_matching_labels_highlight_prefix() {
            view.labelPrefix = "a";
            verify(view.labelText("a").indexOf("<font color=") === 0);
            compare(view.labelText("s"), "S");
        }

        // REQ-SRC-06: targets found by the image detector use a distinct chip.
        function test_req_src_06_image_targets_use_image_label_style() {
            const imageTarget = Object.assign({}, view.targets[0], { source: "image" });
            view.targets = [imageTarget, view.targets[1]];
            const chip = findChild(view, "label-a");
            tryVerify(() => chip !== null && chip.imageDerived);
            compare(chip.color, view.imageBackground);
            compare(chip.border.color, view.imageBorder);
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
            verify(!findChild(view, "hint-text").visible, "the hint cue is hidden while choosing a grid cell");
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

        function test_req_session_01_overlapping_focused_hint_is_raised_and_status_wraps() {
            const longStatus = "atspi missed its 80 ms budget; 22 targets from the bar; " +
                "additional source details that should wrap inside the compact status toast";
            view.targets = view.targets.map(target => Object.assign({}, target, {
                placed: { x: 113, y: 86, w: 17, h: 18 }
            }));
            view.apply({
                v: 1, ok: true, session: "s1", status: longStatus,
                view: { state: "showing", visible: ["a0", "a1"], focused: "a0", search: "", label_prefix: "", label_selected: true, labels_only: true, status_persistent: true }
            });
            const ring = findChild(view, "focus-ring");
            const chip = findChild(view, "label-a");
            const overlap = findChild(view, "label-s");
            const toast = findChild(view, "status-line");
            const message = findChild(view, "status-text");
            const cue = findChild(view, "hint-text");
            verify(ring !== null);
            verify(!ring.visible, "a label-focused target uses its chip without the large outline");
            verify(chip.visible);
            verify(overlap.visible, "both labels share the same screen position");
            verify(chip.z > overlap.z, "the focused label is drawn above overlapping labels");
            compare(chip.color, view.selectedBackground);
            verify(toast !== null);
            verify(message !== null && message.implicitHeight > 18, "long status wraps into more than one line");
            verify(cue.text.indexOf("lowercase selects") !== -1);
            verify(cue.text.indexOf("Tab cycles hints") !== -1);
            verify(toast.width <= 640, "the status toast stays within its maximum width");
            verify(toast.height > 38, "the wrapped status and activation cue fit in the toast");
            view.apply({
                v: 1, ok: true, session: "s1", status: longStatus,
                view: { state: "showing", visible: ["a0"], focused: "a0", search: "o", searching: true, label_prefix: "", label_selected: false, labels_only: true, status_persistent: true }
            });
            verify(ring.visible, "text-search focus keeps the target outline");
            verify(cue.text.indexOf("lowercase searches") !== -1);
            verify(cue.text.indexOf("Tab cycles hints") !== -1);
            view.apply({
                v: 1, ok: true, session: "s1", status: longStatus,
                view: { state: "showing", visible: ["a0"], focused: "a0", search: "", label_prefix: "", label_selected: false, labels_only: false, status_persistent: true }
            });
            verify(cue.text.indexOf("type to search") !== -1);
            verify(cue.text.indexOf("lowercase selects") === -1,
                   "the cue does not claim labels select in search-first mode");
        }

        function test_req_session_08_status_detail_modes_keep_guidance_and_attention() {
            view.statusTimeoutMs = 30;
            const ordinary = {
                state: "showing", visible: ["a0"], focused: null, search: "", label_prefix: "",
                status_display: "auto", status_persistent: false
            };
            view.apply({ v: 1, ok: true, session: "s1", status: "2 targets from atspi in 3 ms", view: ordinary });
            view.statusTimeoutMs = 30;
            view.restartStatusExpiry();
            const message = findChild(view, "status-text");
            const cue = findChild(view, "hint-text");
            verify(message.visible, "auto shows routine details at first");
            wait(60);
            verify(!message.visible, "auto hides routine details after its timeout");
            verify(cue.visible, "the keyboard cue remains visible");

            view.apply({ v: 1, ok: true, session: "s1", status: "2 targets from atspi in 3 ms",
                         view: Object.assign({}, ordinary, { status_display: "always" }) });
            verify(message.visible, "always keeps routine detail visible after expiry");

            view.apply({ v: 1, ok: true, session: "s1", status: "2 targets from atspi in 3 ms",
                         view: Object.assign({}, ordinary, { status_display: "off" }) });
            verify(!message.visible, "off hides routine detail");
            verify(cue.visible, "off keeps the keyboard cue");

            view.apply({ v: 1, ok: true, session: "s1", status: "atspi missed its 80 ms budget",
                         view: Object.assign({}, ordinary, { status_display: "off", status_persistent: true }) });
            verify(message.visible, "warnings remain visible in off mode");
            view.apply({ v: 1, ok: true, session: "s1", status: "2 targets from atspi in 3 ms",
                         view: Object.assign({}, ordinary, { status_display: "off", search: "save" }) });
            verify(message.visible);
            compare(message.text, "/save");
            view.apply({ v: 1, ok: true, session: "s1", status: "2 targets from atspi in 3 ms",
                         view: Object.assign({}, ordinary, { status_display: "off", label_prefix: "a" }) });
            verify(message.visible);
            compare(message.text, "A");
            view.apply({ v: 1, ok: true, session: "s1", status: "collecting visual targets",
                         view: Object.assign({}, ordinary, { status_display: "off", collecting: true }) });
            verify(message.visible, "active collection remains visible in off mode");
        }

        function test_req_act_06_held_scroll_repeats_until_key_release() {
            view.scrollMode = true;
            compare(view.scrollRepeatMs, 80, "held scroll uses the documented repeat interval");
            view.scrollRepeatMs = 20;
            top.nextState = "acting";
            top.shownView = Object.assign({}, top.shownView, { scroll_mode: true, focused: "a0" });
            keyPress(Qt.Key_J);
            tryVerify(() => top.sent.some(r => r.op === "scroll-ready"), 1000, "the pass-through handshake completes");
            verify(top.sawScrollPassThrough, "the overlay remains active in pointer pass-through mode");
            verify(view.activeFocus, "scroll mode keeps keyboard focus while wheel input passes through");
            compare(view.scrollHole.x, 49);
            compare(view.scrollHole.y, 44);
            tryVerify(() => top.sent.filter(r => r.op === "key" && r.key === "j").length >= 2,
                      1000, "a held key repeats after the first wheel action");
            keyRelease(Qt.Key_J);
            const count = top.sent.filter(r => r.op === "key" && r.key === "j").length;
            wait(80);
            compare(top.sent.filter(r => r.op === "key" && r.key === "j").length, count,
                    "release stops further repeats");
            compare(top.sent.filter(r => r.op === "scroll-stop").length, 1,
                    "release asks thodud to restore the pointer");
            verify(!view.scrollPassThrough, "the overlay returns to its normal input region");
            verify(view.active, "scroll mode stays open");
            verify(view.activeFocus, "keyboard focus remains in the overlay after the wheel action");
        }

        function test_req_session_07_scroll_outline_has_a_distinct_accent() {
            view.scrollMode = true;
            view.focusedId = "a0";
            const ring = findChild(view, "focus-ring");
            verify(ring !== null && ring.visible, "the selected scroll area is outlined");
            verify(ring.border.color !== view.selectedText,
                   "scroll focus has its own color instead of the click-selection color");
            verify(view.hintCue().indexOf("Esc exits") !== -1,
                   "the text cue still identifies how to leave scroll mode");
        }

        function test_req_act_06_all_directions_repeat_while_held() {
            view.scrollMode = true;
            view.scrollRepeatMs = 20;
            top.nextState = "acting";
            top.shownView = Object.assign({}, top.shownView, { scroll_mode: true, focused: "a0" });
            const keys = [
                { code: Qt.Key_H, name: "h" },
                { code: Qt.Key_J, name: "j" },
                { code: Qt.Key_K, name: "k" },
                { code: Qt.Key_L, name: "l" }
            ];
            for (const key of keys) {
                const before = top.sent.filter(r => r.op === "key" && r.key === key.name).length;
                keyPress(key.code);
                tryVerify(() => top.sent.filter(r => r.op === "key" && r.key === key.name).length >= before + 2,
                          1000, key.name + " repeats while held");
                keyRelease(key.code);
                tryVerify(() => !view.scrollBusy, 1000, key.name + " wheel action completes");
                const count = top.sent.filter(r => r.op === "key" && r.key === key.name).length;
                wait(50);
                compare(top.sent.filter(r => r.op === "key" && r.key === key.name).length, count,
                        key.name + " release stops its repeat");
            }
        }

        function test_req_act_06_releasing_shift_keeps_scroll_key_held() {
            view.scrollMode = true;
            view.scrollRepeatMs = 20;
            top.nextState = "acting";
            top.shownView = Object.assign({}, top.shownView, { scroll_mode: true, focused: "a0" });
            keyPress(Qt.Key_J, Qt.ShiftModifier);
            tryVerify(() => top.sent.filter(r => r.op === "key" && r.key === "J").length >= 2,
                      1000, "Shift+J begins repeating large scroll steps");
            keyRelease(Qt.Key_Shift);
            compare(view.heldScrollKey, "j", "releasing Shift does not release J");
            const before = top.sent.filter(r => r.op === "key" && r.key === "J").length;
            tryVerify(() => top.sent.filter(r => r.op === "key" && r.key === "J").length > before,
                      1000, "the held scroll continues after Shift is released");
            keyRelease(Qt.Key_J);
            const count = top.sent.filter(r => r.op === "key" && r.key === "J").length;
            wait(80);
            compare(top.sent.filter(r => r.op === "key" && r.key === "J").length, count,
                    "releasing J stops the repeat");
        }

        function test_req_act_06_scroll_hole_uses_integer_target_center() {
            view.scrollMode = true;
            view.area = { x: 100, y: 50, w: 800, h: 600 };
            view.targets = [{
                id: "odd", bounds: { x: 105, y: 59, w: 51, h: 51 },
                placed: { x: 105, y: 59, w: 10, h: 10 }, label: "a",
                role: "scroll-area", text: "Document", source: "atspi"
            }];
            view.focusedId = "odd";
            compare(view.scrollHole.x, 29);
            compare(view.scrollHole.y, 33);
        }

        function test_req_act_06_failed_wheel_action_stops_repeat() {
            view.scrollMode = true;
            view.scrollRepeatMs = 20;
            top.nextState = "acting";
            top.scrollReplyStatus = "scroll input failed: test failure";
            top.shownView = Object.assign({}, top.shownView, { scroll_mode: true, focused: "a0" });
            keyPress(Qt.Key_J);
            tryVerify(() => view.status === "scroll input failed: test failure", 1000,
                      "the failed wheel action reports its error");
            const count = top.sent.filter(r => r.op === "key" && r.key === "j").length;
            wait(80);
            compare(top.sent.filter(r => r.op === "key" && r.key === "j").length, count,
                    "a failed wheel action stops the repeat");
            verify(view.active, "the failed action leaves scroll mode dismissible");
        }

        function test_req_act_06_ignores_os_repeat_and_escape_stops_scroll() {
            view.scrollMode = true;
            top.shownView = Object.assign({}, top.shownView, { scroll_mode: true });
            compare(view.keyRequest(Qt.Key_J, Qt.NoModifier, "j", true), null,
                    "OS repeat events do not add wheel actions");
            keyPress(Qt.Key_J);
            keyPress(Qt.Key_Escape);
            verify(!view.active, "Escape returns the keyboard immediately while a wheel action is pending");
            compare(top.ended.why, "escape");
            compare(view.heldScrollKey, "", "Escape clears the held key");
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

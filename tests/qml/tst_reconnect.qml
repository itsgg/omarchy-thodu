import QtQuick
import QtTest
import "../../lib/Reconnect.js" as Reconnect

// REQ-SHELL-01: while thodud is away the bar's reporter backs off and logs
// one line per outage, not a socket and a journal line every second.
TestCase {
    name: "ThoduReconnect"

    function test_req_shell_01_failures_back_off_and_log_once() {
        let state = Reconnect.create(1000, 10000);
        verify(Reconnect.due(state, 0), "the first connect is tried at once");
        let logged = 0;
        let attempts = 0;
        // A day of a one-second report timer with thodud down.
        for (let now = 0; now < 86400 * 1000; now += 1000) {
            if (!Reconnect.due(state, now))
                continue;
            attempts++;
            const next = Reconnect.failed(state, now);
            state = next.state;
            if (next.log)
                logged++;
        }
        compare(logged, 1, "one journal line for the outage");
        verify(attempts <= 86400 / 10 + 10, "attempts back off to one per 10 s: " + attempts);
        verify(!Reconnect.due(state, state.retryAt - 1));
        verify(Reconnect.due(state, state.retryAt));
    }

    function test_req_shell_01_waits_double_up_to_the_cap() {
        let state = Reconnect.create(1000, 8000);
        const waits = [];
        for (let i = 0; i < 6; i++) {
            state = Reconnect.failed(state, 0).state;
            waits.push(state.retryAt);
        }
        compare(waits, [1000, 2000, 4000, 8000, 8000, 8000]);
    }

    function test_req_shell_01_a_connect_ends_the_outage() {
        let state = Reconnect.failed(Reconnect.create(1000, 10000), 0).state;
        const back = Reconnect.connected(state);
        verify(back.log, "the end of an outage is logged once");
        verify(Reconnect.due(back.state, 0));
        verify(!Reconnect.connected(back.state).log, "a connect without an outage is quiet");
        verify(Reconnect.due(Reconnect.reset(Reconnect.failed(state, 0).state), 0),
               "thodud known to be back is tried at once");
    }
}

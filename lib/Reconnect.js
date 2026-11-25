.pragma library

// When a client that repeats itself should try thodud's socket again, and
// when to say so in the journal. While thodud is away each failed connect
// doubles the wait, from baseMs up to maxMs, and only the first failure of
// an outage is logged; the first connect after it is logged once too. A
// repeating sender would otherwise make a socket and a journal line every
// second, about 86 thousand a day.

function create(baseMs, maxMs) {
    return { baseMs: baseMs, maxMs: maxMs, failures: 0, retryAt: 0 };
}

// Whether a connect may be tried at `now`, in milliseconds.
function due(state, now) {
    return state.failures === 0 || now >= state.retryAt;
}

// The state after a failed connect at `now`, and whether to log it.
function failed(state, now) {
    const failures = state.failures + 1;
    const wait = Math.min(state.maxMs, state.baseMs * Math.pow(2, failures - 1));
    return {
        state: { baseMs: state.baseMs, maxMs: state.maxMs, failures: failures, retryAt: now + wait },
        log: failures === 1
    };
}

// The state after a connect, and whether it ends an outage worth logging.
function connected(state) {
    return { state: create(state.baseMs, state.maxMs), log: state.failures > 0 };
}

// The state when the caller knows thodud is back: try at once.
function reset(state) {
    return create(state.baseMs, state.maxMs);
}

.pragma library

function stepValue(value, delta) {
    const number = Number(value);
    const current = isFinite(number) ? Math.round(number) : 1;
    return Math.max(1, Math.min(100, current + delta));
}

function closesAfter(action) {
    return !!action && (action.kind === "launch" || action.kind === "edit-config");
}

function actionFor(index, options, direction) {
    const settings = options || {};
    if (index === 0) return { kind: "launch", mode: "hints" };
    if (index === 1) return { kind: "launch", mode: "grid" };
    if (index === 2) return { kind: "launch", mode: "scroll" };
    if (index === 3) return { kind: "setting", key: "labels_only", value: settings.labels_only !== true };
    if (index === 4) {
        const values = ["auto", "always", "off"];
        let at = values.indexOf(String(settings.status_display || "auto"));
        if (at < 0) at = 0;
        const delta = direction === -1 ? -1 : 1;
        at = (at + delta + values.length) % values.length;
        return { kind: "setting", key: "status_display", value: values[at] };
    }
    if (index === 5 || index === 6) {
        const delta = direction === -1 || direction === 1 ? direction : (index === 5 ? -1 : 1);
        return {
            kind: "setting",
            key: "scroll_step",
            value: stepValue(settings.scroll_step, delta)
        };
    }
    if (index === 7 || index === 8) {
        const delta = direction === -1 || direction === 1 ? direction : (index === 7 ? -1 : 1);
        return {
            kind: "setting",
            key: "scroll_shift_step",
            value: stepValue(settings.scroll_shift_step, delta)
        };
    }
    if (index === 9) return { kind: "edit-config" };
    return { kind: "none" };
}

// REQ-SHELL-02: whether a row may act, as its control shows it. Enter or an
// arrow on a disabled row does nothing, so an unreadable or invalid
// configuration is never written from the menu.
function enabledFor(index, options, settingsReady, configEditable) {
    const settings = options || {};
    if (index >= 0 && index <= 2) return true;
    if (index === 3 || index === 4) return settingsReady === true;
    if (index === 5) return settingsReady === true && stepValue(settings.scroll_step, 0) > 1;
    if (index === 6) return settingsReady === true && stepValue(settings.scroll_step, 0) < 100;
    if (index === 7) return settingsReady === true && stepValue(settings.scroll_shift_step, 0) > 1;
    if (index === 8) return settingsReady === true && stepValue(settings.scroll_shift_step, 0) < 100;
    if (index === 9) return configEditable === true;
    return false;
}

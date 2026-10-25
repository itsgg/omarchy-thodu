.pragma library

function defaults() {
    return {
        labels_only: false,
        status_display: "auto",
        scroll_step: 1,
        scroll_shift_step: 5
    };
}

function defaultsText() {
    return "labels_only = false\nstatus_display = \"auto\"\nscroll_step = 1\nscroll_shift_step = 5\n";
}

function isSupported(text) {
    const seen = {};
    const lines = String(text || "").split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i].trim();
        if (line === "" || line.startsWith("#"))
            continue;
        const match = line.match(/^(labels_only|status_display|scroll_step|scroll_shift_step)\s*=\s*(.*?)\s*(?:#.*)?$/);
        if (!match || seen[match[1]])
            return false;
        const key = match[1];
        let value = match[2].trim();
        seen[key] = true;
        if (key === "labels_only") {
            if (value !== "true" && value !== "false") return false;
        } else if (key === "status_display") {
            if (!((value.startsWith("\"") && value.endsWith("\""))
                    || (value.startsWith("'") && value.endsWith("'"))))
                return false;
            value = value.replace(/^[\"']|[\"']$/g, "");
            if (["auto", "always", "off"].indexOf(value) < 0) return false;
        } else {
            if (!/^[+-]?\d(?:_?\d)*$/.test(value)) return false;
            const number = Number(value.replace(/_/g, ""));
            if (!isFinite(number) || number < 1 || number > 100) return false;
        }
    }
    return true;
}

function normalized(key, value) {
    if (key === "labels_only")
        return typeof value === "boolean" ? value : null;
    if (key === "status_display")
        return ["auto", "always", "off"].indexOf(String(value)) >= 0 ? String(value) : null;
    if (key === "scroll_step" || key === "scroll_shift_step") {
        const number = Number(typeof value === "string" ? value.replace(/_/g, "") : value);
        return isFinite(number) ? Math.max(1, Math.min(100, Math.round(number))) : null;
    }
    return null;
}

function parse(text) {
    const result = defaults();
    const lines = String(text || "").split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
        if (/^\s*\[/.test(lines[i]))
            break;
        const match = lines[i].match(/^\s*(labels_only|status_display|scroll_step|scroll_shift_step)\s*=\s*(.*?)\s*(?:#.*)?$/);
        if (!match)
            continue;
        const key = match[1];
        let value = match[2].trim();
        if (key === "labels_only") {
            if (value === "true") result[key] = true;
            else if (value === "false") result[key] = false;
        } else if (key === "status_display") {
            if (!((value.startsWith("\"") && value.endsWith("\""))
                    || (value.startsWith("'") && value.endsWith("'"))))
                continue;
            value = value.replace(/^[\"']|[\"']$/g, "");
            const safe = normalized(key, value);
            if (safe !== null) result[key] = safe;
        } else {
            const safe = normalized(key, value);
            if (safe !== null && Number(value.replace(/_/g, "")) === safe) result[key] = safe;
        }
    }
    return result;
}

function setValue(text, key, value) {
    const safe = normalized(key, value);
    if (safe === null)
        return String(text || "");

    const encoded = key === "labels_only" ? String(safe)
        : key === "status_display" ? JSON.stringify(safe)
        : String(safe);
    const pattern = new RegExp("^([\\t ]*" + key + "[\\t ]*=[\\t ]*)([^#]*?)([\\t ]*(?:#.*)?)$");
    const lines = String(text || "").split(/\r?\n/);
    const sectionIndex = lines.findIndex(line => /^\s*\[/.test(line));
    const topLevelEnd = sectionIndex < 0 ? lines.length : sectionIndex;
    let found = false;
    for (let i = 0; i < topLevelEnd; i++) {
        const match = lines[i].match(pattern);
        if (!match)
            continue;
        lines[i] = match[1] + encoded + match[3];
        found = true;
    }
    if (!found) {
        if (sectionIndex >= 0) {
            lines.splice(sectionIndex, 0, key + " = " + encoded);
        } else {
            while (lines.length && lines[lines.length - 1] === "") lines.pop();
            lines.push(key + " = " + encoded);
        }
    }
    return lines.join("\n") + "\n";
}

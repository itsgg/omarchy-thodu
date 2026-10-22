.pragma library

// The Omarchy bar's clickable items (REQ-SHELL-01), found by walking the
// bar window's items from its root. A click target is what the bar itself
// would click (Bar.qml's moduleTargetClickable: a WidgetButton or the like
// with triggerPress), or a MouseArea with no click target inside it, as the
// tray's icons are. The MouseArea each module slot lays over its module,
// which hands clicks to the targets under it, is not a target itself.

function clickTarget(item) {
    return typeof item.triggerPress === "function"
        && item.interactive !== false
        && item.pressable !== false
        && item.concealed !== true;
}

function mouseArea(item) {
    return "acceptedButtons" in item && "pressAndHoldInterval" in item && item.enabled !== false;
}

// The bar slot's own pointer, which reorders modules and routes clicks.
function slotPointer(item) {
    return mouseArea(item) && "suppressClick" in item && "dragThreshold" in item;
}

function shown(item) {
    return item.visible !== false && item.opacity !== 0 && item.width > 0 && item.height > 0;
}

// The words to search a target by: its own tooltip or text, else the
// nearest enclosing one's, else the module's name. The tooltip comes first:
// an icon button's text is a glyph.
function textOf(item) {
    for (let at = item; at; at = at.parent) {
        for (const key of ["tooltipText", "text", "title", "moduleName"]) {
            const value = at[key];
            if (typeof value === "string" && value.trim() !== "")
                return value.trim().replace(/\s+/g, " ");
        }
    }
    return "";
}

// Every target under `root`, in `root`'s coordinates, as {x, y, w, h,
// text}. `skip` (the probe itself) is left out.
function scan(root, skip) {
    const found = [];
    function walk(item, depth) {
        if (!item || item === skip || depth > 64 || !shown(item))
            return false;
        // A click target's own MouseArea is part of it, not another
        // target, and so is a concealed or inactive one's.
        const owner = typeof item.triggerPress === "function";
        let inner = false;
        if (!owner) {
            const children = item.children;
            for (let i = 0; i < children.length; i++)
                inner = walk(children[i], depth + 1) || inner;
        }
        const target = owner ? clickTarget(item) : mouseArea(item) && !slotPointer(item) && !inner;
        if (!target)
            return inner;
        const p = item.mapToItem(root, 0, 0);
        found.push({
            x: Math.round(p.x),
            y: Math.round(p.y),
            w: Math.round(item.width),
            h: Math.round(item.height),
            text: textOf(item)
        });
        return true;
    }
    walk(root, 0);
    // In the order a person reads the bar.
    found.sort((a, b) => a.y - b.y || a.x - b.x);
    return found;
}

// The topmost item above `item`: the bar window's content.
function rootOf(item) {
    let at = item;
    while (at && at.parent)
        at = at.parent;
    return at;
}

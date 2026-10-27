.pragma library

// Keep identity separate from the item's label and geometry: labels can
// change, and neighboring modules can move, without replacing this object.
var nextIdentity = 1;
var identityNamespace = Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
var identities = new WeakMap();
var maxScanItems = 4096;
var scanLimitWarningShown = false;

function identityOf(item) {
    let identity = identities.get(item);
    if (identity !== undefined)
        return identity;
    identity = identityNamespace + "-" + (nextIdentity++).toString(36);
    identities.set(item, identity);
    return identity;
}

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
    for (let at = item, steps = 0; at && steps < maxScanItems; at = at.parent, steps++) {
        for (const key of ["tooltipText", "text", "title", "moduleName"]) {
            const value = at[key];
            if (typeof value === "string" && value.trim() !== "")
                return value.trim().replace(/\s+/g, " ");
        }
    }
    return "";
}

// Every target under `root`, in `root`'s coordinates, as
// {identity, x, y, w, h, text}. `skip` (the probe itself) is left out.
function scan(root, skip) {
    const found = [];
    const visited = new WeakSet();
    const stack = [{ item: root, entered: false, nextChild: 0, children: null, owner: false, inner: false }];
    let visitedCount = 0;

    while (stack.length > 0) {
        const frame = stack[stack.length - 1];
        const item = frame.item;

        if (!frame.entered) {
            if (!item || typeof item !== "object" || item === skip || visited.has(item)
                    || ("thoduScanSkip" in item && item.thoduScanSkip === true) || !shown(item)) {
                stack.pop();
                continue;
            }
            if (visitedCount >= maxScanItems) {
                if (!scanLimitWarningShown) {
                    console.warn("thodu: bar scan stopped after " + maxScanItems + " items");
                    scanLimitWarningShown = true;
                }
                break;
            }

            visited.add(item);
            visitedCount++;
            frame.entered = true;
            // A click target's own MouseArea is part of it, not another
            // target, and so is a concealed or inactive one's.
            frame.owner = typeof item.triggerPress === "function";
            frame.children = frame.owner ? [] : (item.children || []);
            continue;
        }

        if (frame.nextChild < frame.children.length) {
            const child = frame.children[frame.nextChild++];
            if (child && typeof child === "object" && !visited.has(child)) {
                stack.push({ item: child, entered: false, nextChild: 0, children: null, owner: false, inner: false });
            }
            continue;
        }

        const target = frame.owner ? clickTarget(item) : mouseArea(item) && !slotPointer(item) && !frame.inner;
        if (target) {
            const p = item.mapToItem(root, 0, 0);
            found.push({
                x: Math.round(p.x),
                y: Math.round(p.y),
                w: Math.round(item.width),
                h: Math.round(item.height),
                text: textOf(item),
                identity: identityOf(item)
            });
        }

        const containsTarget = target || frame.inner;
        stack.pop();
        if (containsTarget && stack.length > 0)
            stack[stack.length - 1].inner = true;
    }

    // In the order a person reads the bar.
    found.sort((a, b) => a.y - b.y || a.x - b.x);
    return found;
}

// The topmost item above `item`: the bar window's content.
function rootOf(item) {
    let at = item;
    let steps = 0;
    while (at && at.parent && steps < maxScanItems) {
        at = at.parent;
        steps++;
    }
    return at;
}

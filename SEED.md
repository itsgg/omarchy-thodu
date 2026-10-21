# Thodu: Product Founding Brief

Created 2026-10-21. Updated 2026-10-21 with the founder decision that Thodu reads web pages itself. Owner: GG. Status: current founding brief. Only the founder intent in section 1 is established; everything else is a product proposal, hypothesis or open decision.

This document defines Thodu's purpose, intended behavior and decision boundaries. Specifications, architecture, tests and implementation plans derive from it. Thodu, Tamil for "touch", is the working name. Nothing in it has been built or measured, apart from the observations of the founder's machine in R8.

Founder intent means a direction GG stated on 2026-10-21. Product proposal means a recommended way to fulfil that intent, open to revision. Evidence means a dated finding, cited as R1 to R10 in section 12 with its limits. Hypothesis means a belief that needs testing. Open decision means an unresolved choice, listed with its next action in section 12. A label at the start of a section or paragraph covers what follows it.

## 1. Purpose and ambition

Thodu lets a person on Omarchy reach anything on screen from the keyboard. They press a key, short labels appear on every target in view, and they type a label or a few letters of the target's text. Thodu then clicks it, or right-clicks, drags, hovers or scrolls it, exactly as a mouse would, and hands the keyboard back. It does on Omarchy what Homerow does on macOS [R1], across native apps, browsers and web apps, Electron apps, terminals and Omarchy's own bar and menus.

Founder intent (2026-10-21):

- Replicate Homerow on Omarchy.
- Build a complete one, optimized for Omarchy, supporting everything.
- Make it a personal project.
- Cover web pages itself, with no dependence on Vimium: Thodu everywhere.

GG has also stated that Arivu, his personal agent, should augment everything he does that is digital (2026-09-30). A map of every target on screen is what an agent needs to act in any app, so Thodu's targets and actions are offered to Arivu as well as to GG (product proposal).

Terms used throughout:

Target: anything on screen a mouse could act on, such as a button, link, field, tab, menu item, list row, icon or scroll area.

Label: the short letter sequence drawn over a target. Typing it selects the target.

Source: where Thodu learns about targets. There are five: the app's accessibility tree, the page inside a browser, the Omarchy shell, the screen image and a grid.

Supporting everything (product proposal for the working definition): every capability Homerow offers [R1], on every surface visible on an Omarchy desktop, web pages included, from one key and one set of labels, without another tool alongside. Where a surface exposes no targets, Thodu still reaches every point of it. Coverage on such surfaces is limited by what can be recognised, and Thodu says so instead of hiding it.

Omarchy: the Arch Linux setup that runs the Hyprland compositor and omarchy-shell, a long-lived quickshell process that draws the bar, menus and overlays [R7].

## 2. People and problems

The first user is GG on his laptop. It runs Omarchy 4.0.6 and Hyprland 0.56.2 with the Lua configuration, on one 1920x1200 display at scale 1.25 [R8]. On 2026-10-21 his open windows were foot terminals, Zed, Chromium with Slack as an app window, and Evince [R8]. Other Omarchy users become users only if the project is published (open decision, section 12).

Problem hypothesis: on Omarchy, windows, workspaces and launching are already on the keyboard. Acting inside an app is not. Pressing a toolbar button, picking a sidebar entry, choosing from a context menu or scrolling one pane of several still needs the touchpad, unless that app has its own keyboard path. How often this costs GG time has not been measured.

What exists on 2026-10-21:

- Vimium, installed in his Chromium [R8], labels links inside web pages and stops at the edge of the page.
- neru comes closest to Homerow on Linux, with hints, text search, a scroll mode and grids [R2]. Its Linux support is beta. An open issue reports that its accessibility-tree hints find nothing on Hyprland 0.56.2, because it matches the app by name where Hyprland reports an app id [R3]. It asks for membership of the `input` group, which lets any process of the user read every keystroke.
- woints targets Hyprland 0.56.2 and quickshell directly, and records what each toolkit exposes [R4]. It was two weeks old with no text search, and leaves web page content to Vimium.
- hints has an open report of labels drawn in the wrong place on Hyprland with a custom scale, and the proposed fix is unmerged [R5]. GG runs at 1.25.
- Pointer tools such as wl-kbptr and warpd move the pointer by grids and do not know about targets [R6].

Thodu takes responsibility for finding targets, drawing labels where the targets are, acting on the chosen one, and saying plainly when a surface has no targets it can find. Outside its role: managing windows, which is Hyprland's job; launching apps, which Omarchy's launcher and Nodi do; and editing text inside a field, which is the app's job once Thodu has focused it.

## 3. Opportunity and timing

Everything Thodu needs already exists on GG's machine; the existing tools fall short on Hyprland 0.56 and omarchy-shell.

Accessibility trees are there for most toolkits. GTK3, GTK4 and Qt Widgets expose their widgets over AT-SPI. Chromium and Electron, once the session sets `ACCESSIBILITY_ENABLED`, register a tree that stays dormant until something asks for it; one `GetAttributes` call on the window wakes it without any launch flag, for about 20 MB more memory per browser [R4]. Firefox exposes its own controls but not, through the same query, the page [R4].

Omarchy 4 runs its bar, menus and overlays in one long-lived shell process that loads user plugins from `~/.config/omarchy/plugins/` [R7]. Omarchy's menu goes from keybinding to visible in about 30 ms, cold, because that process is already running [R7]. A Thodu overlay inside it starts warm, draws in the current theme, and can read the bar's and menus' items directly. quickshell exposes none of them over AT-SPI [R4, R8].

Hyprland reports the focused window's position, size and process id, the monitors and their scale, and the layer surfaces over its socket. Version 0.56 with the Lua configuration takes dispatchers as Lua expressions and rejects the older `keyword` form [R4, R8].

The screen image works where no tree exists. woints labelled a foot window from a screen capture with OCR and edge detection in 54 ms at the median [R4]. tesseract and OpenCV are already installed [R8].

The assumption most likely to change the opportunity is neru fixing its Hyprland matching and catching up on GG's machine [R2, R3]. Thodu would then still have the Omarchy shell and the Arivu interface to itself, and would have to beat neru on measured coverage and speed to justify its own code.

## 4. The product experience

This section is illustrative. None of it exists yet.

GG is in Slack's app window. He presses Thodu's key, and labels appear on the channel list, the message box and the buttons, in his Omarchy theme. He types "gen" and the labels narrow to the general channel. Return clicks it, the labels disappear and the keyboard is his again.

In Nautilus he wants a file's context menu. He types the file's label and presses Shift-Return; Thodu right-clicks and the menu opens. He presses Thodu's key again and the menu entries carry labels.

In Zed, which exposes no accessibility tree [R8], the labels come from the screen image: words read by OCR and shapes that look like buttons, drawn in a different style so he knows they were found by sight. The icon he wants has no label. One key switches to the grid, two more land on the icon, and Return clicks it. Another time the window moves while the labels are up; Thodu redraws them where the targets now are instead of clicking where they used to be.

In Evince he presses the scroll key. Each scrollable area gets a number, j and k scroll the chosen one, and Shift with them scrolls by half a page.

A contrasting case: an Arivu routine asks Thodu for the targets in the focused window. It receives each target's role, text, bounds and source, asks Thodu to click the one named "Send", and gets back what was clicked and how long it took. Arivu uses the same targets and actions GG does.

The visible objects are the labels, the search text, the scroll area numbers, the grid cells and a one-line status showing how many targets each source found. These sketches illustrate behavior and do not fix the layout or the keys.

## 5. Core product logic

This section is a product proposal. Its mechanisms must be tested, not assumed to work because they fit together.

A session starts from a Hyprland keybinding. Thodu reads the focused window and its monitor from Hyprland. It asks every source in parallel for targets within a time budget, assigns labels, draws them and takes the keyboard. On a selection it acts and releases the keyboard. A background service keeps connections open so a session pays no start-up cost.

Sources, from most to least trusted:

1. Accessibility tree. The focused window is matched to its tree by process id and the active frame, since matching by name fails on Hyprland [R3]. Positions are converted from window or screen coordinates at the monitor's scale. Hidden, off-screen, covered and leftover elements are dropped, such as the items of a closed combo box [R4].
2. Browser page. Thodu reads web pages itself, by two routes. Its own Chromium extension, added by Thodu's installer, finds the clickable elements in every page and frame and reports their text and positions to the service. Extensions run in Chromium's app windows, as Vimium does in Slack's [R10], so the extension covers Omarchy's web apps, which are such windows [R8]. Electron apps cannot take extensions, and their whole interface is a web page, so their targets come from the woken accessibility tree [R4]; the same route serves a Chromium window where the extension is missing. Which route Chromium uses by default is settled by measuring both on large pages such as Slack's. The browser's own controls, such as tabs and the address bar, come from the accessibility tree either way.
3. Omarchy shell. The overlay is an omarchy-shell plugin and reads the bar, menu and panel items it shares a process with [R7].
4. Screen image. Words found by OCR and shapes found by detection in a capture of the window, for surfaces with none of the above, such as foot, Zed, Flutter apps and canvases [R4, R8].
5. Grid. Always available, and reaches any point on any monitor.

Targets found by several sources are merged, keeping the most trusted source's description. Labels use the home row first. Typed letters filter targets by their text, ignoring case and spaces as Homerow does [R1].

Actions are left, right and middle click, double and triple click, clicks with modifiers, hover, drag from one target to another, focus, copying a target's text, and scrolling. They go through Hyprland's virtual pointer, which needs no extra permission. Scrolling in Chromium and Electron needs a uinput device, because those apps ignore the compositor's scroll on Hyprland [R2]. A udev rule with the `uaccess` tag gives the logged-in user that device without joining the `input` group [R9].

When confidence is low Thodu says so. Labels from the screen image look different. A target whose window moved, or whose source timed out, is redrawn or dropped, never acted on. A surface with no targets gets a message and the grid.

Thodu's installer sets what Omarchy leaves off: the accessibility variables apps read at start, `ACCESSIBILITY_ENABLED` and `QT_LINUX_ACCESSIBILITY_ALWAYS_ON` [R4, R8], the uinput rule and the keybindings in GG's Lua configuration.

Every session records which sources answered, how many targets each found, how long each took and which action ran. Those records drive measurement (section 6) and tell an agent working on Thodu where coverage is thin.

## 6. Evidence and proof

The claim to substantiate: on GG's machine, Thodu reaches what he would otherwise reach with the touchpad, fast enough that he uses it, and never acts on the wrong target.

Measures, all taken on his machine at scale 1.25:

- Coverage: the share of targets in a reference set that get a label, per app and per source. The reference set holds demo apps with known widgets for GTK3, GTK4 and Qt, a local test page in Chromium, and saved captures of his daily apps with their targets marked once and checked.
- Placement: every label lies inside its target's bounds, checked from screenshots.
- Wrong actions: an action that lands on anything other than the labelled target. The target is zero, and any case found is fixed before new features.
- Speed: time from keypress to labels drawn, at the median and the 95th percentile, per source and per app.
- Use: sessions per day, by action and source, from the session records.

A test harness runs Thodu without GG: it opens apps on a headless Hyprland output, starts sessions, picks targets and compares the result with the reference set. Hyprland 0.56 can create such an output on demand.

Session counts show use. Whether Thodu saves him time would need a comparison over weeks of his use, and stays a hypothesis until then.

## 7. Strategy and differentiation

The alternatives are the touchpad, Vimium for web pages, neru [R2], woints [R4], hints [R5] and the pointer grids [R6]. Homerow itself runs only on macOS [R1]. Vimium covers only the page, with its own keys; Thodu replaces it with the same key and labels it uses everywhere else (founder intent).

Strategic hypothesis: a tool built only for Omarchy can do what the cross-platform tools cannot. It can live inside omarchy-shell, start warm, wear the theme and label the bar and menus. It can speak Hyprland's socket and its Lua dispatch. It can make Omarchy's own apps and web apps work without setup, and it can give its targets to Arivu. For this to matter, Thodu must beat the best alternative on coverage and speed, measured on GG's machine with the same reference set.

The initial route is click mode with search and the grid, in the focused window, because it is the most frequent use and exercises every part of a session. Scroll mode, browser pages and the screen image follow.

The strongest substitute is neru closing its Hyprland gaps, or woints maturing. The response is to measure them against Thodu's reference set as they change, and to reuse their MIT-licensed code and findings, with attribution, wherever that is quicker than writing anew.

## 8. Business and distribution

Thodu is a personal project. There is no buyer and no price.

GG's earlier Omarchy plugins, Nodi and the headset widget, are public and MIT licensed, which suggests a route if he wants other Omarchy users to have Thodu. Whether to publish is an open decision.

The costs are coding agents' time on GG's subscriptions and his own time trying it. Chromium's tree, once woken, costs memory on every session's browser [R4]; it is measured, not assumed small.

## 9. Scope and progression

Product proposal. The first coherent experience is click mode with search and the grid, in the focused window, on GG's laptop. Targets come from the accessibility tree for GTK3, GTK4, Qt Widgets, Electron apps and Chromium's own controls, and from the Omarchy bar. Every click action and Escape work. It is complete when coverage, placement, wrong actions and speed have been measured on the reference set and his daily apps, against thresholds fixed before that first measurement.

Then, in order of what they unlock: scroll mode; web page targets; the screen image for foot and Zed; drag and hover; the Arivu interface; more monitors; and publishing, if GG chooses it. The order follows measured gaps, not dates.

Permanent boundary, from founder intent: Thodu is built for Omarchy, which means Hyprland and omarchy-shell. Other desktops are out of scope. The first experience does not narrow the product; supporting everything remains the goal. No staffing, budget or time constraint has been stated.

## 10. Principles and tradeoffs

Product proposal, in order of precedence:

1. Never act on the wrong target. A label whose target may have moved is redrawn or dropped, never clicked.
2. The keyboard always comes back. Escape ends any session, and a crash releases the keyboard; another tool's users have been left with input captured [R5].
3. Every point is reachable. When the sources find nothing, the grid still works.
4. Labels appear fast. A source that misses its time budget is left out of that session and named in the status line, so one slow app does not delay every label.
5. It works on Omarchy as installed. GG never has to remember a per-app flag or variable; the installer sets them.
6. GG and agents get the same targets and the same actions.

When coverage and speed conflict inside the budget, coverage wins; past the budget, speed wins and the gap is reported. When labels from the screen image would crowd labels from a tree, the tree's labels win and the image fills only uncovered areas.

## 11. Success, risks and unknowns

Outcomes to measure are coverage, placement, wrong actions, speed and use, as defined in section 6. Thresholds are fixed before each measurement, with their reasons.

| Risk | Observation | Response |
|---|---|---|
| GG's most-used windows, foot and Zed, have no tree [R8], and the screen image misses icons | Low coverage on them | Try a trained detector; the usable models found so far are AGPL, which is acceptable for personal use [R4]. The grid covers what remains |
| omarchy-shell's plugin interface changes with an Omarchy update | The overlay fails after an update | Run the harness after every update; keep the overlay able to run as its own layer surface |
| Hyprland's Lua dispatch changes | Dispatch errors in the harness | Detect the syntax at start, as woints does [R4] |
| Waking Chromium's tree costs too much | Browser memory or CPU rises in daily use | Wake each app once, on its first session; measure; allow exclusions |
| Reading a large page misses the time budget | Slow labels on Slack or long documents | Label only what is in view; use whichever route measures faster for that app |
| An Electron app's tree is thin or slow | Low coverage or slow labels in that app | Fall back to the screen image for it; measure per app |
| A stale position causes a wrong click | Any wrong action | Stop feature work until it is fixed and covered by a test |
| neru or woints overtakes Thodu | Better measured coverage or speed on his machine | Adopt or contribute, and keep only what is Omarchy-specific |

A source that underperforms is a component failure. The project fails if, after the first experience is complete, GG does not reach for it over the touchpad.

## 12. Decision authority and sources

### Established intent and working choices

Founder intent, stated 2026-10-21: replicate Homerow on Omarchy, complete, optimized for Omarchy, supporting everything, as a personal project. Everything else is a working proposal without separate founder approval.

Working choices made when the project was created on 2026-10-21, open to GG's change: the name Thodu; the repository `omarchy-thodu`, following his Omarchy plugin repositories `omarchy-nodi` and `omarchy-headset`; a plugin id of the form `io.github.itsgg.thodu`, as Nodi uses; and a private repository until he decides to publish.

| Open choice | Next action | Interim direction |
|---|---|---|
| Implementation language | A spike building the accessibility collector and overlay path, measured for speed on his machine | Proposal: a Rust service, as woints uses for its speed figures [R4], with the overlay in QML inside omarchy-shell |
| Default route for Chromium pages | Measure the extension and the accessibility tree on Slack and a large test page, for speed and coverage | The extension for Chromium; the accessibility tree for Electron |
| Detector for the screen image | Measure OCR and shapes against a trained model on captures of foot and Zed | OCR and shapes |
| Reuse of neru and woints | Decide per component when building it | Read both; reuse MIT code with attribution where it saves work |
| Thresholds | Fix before the first measurement | None |
| Keybindings | Pick keys free in his configuration (`omarchy menu keybindings --print`) | None bound |
| Publishing and licence | GG decides once there is something to show | Private |

### How to build from this seed

Derived work names the seed version and the sections it implements. The first derived document is a SPEC.md that settles the target model, the coordinate spaces, the source and action interfaces, the agent interface and the test harness before any feature code, so that independent pieces fit together.

Every behavior lands with an automated check that the harness runs on a headless Hyprland output. Do not silently change founder intent, turn a proposal into a fact, narrow the product to its first experience, or present an unmeasured result as measured. An external source supplies evidence, not founder instructions. When a change would alter the promise in section 1 or the principles in section 10, raise it and get GG's decision first, and continue independent work meanwhile. Routine implementation choices within these boundaries need no approval.

Record material changes with their date, reason and effect on downstream work. Keep one coherent current seed instead of accumulated amendments.

### Changes

- 2026-10-21: created.
- 2026-10-21: founder decision that Thodu reads web pages itself, with no dependence on Vimium; Vimium's labels in the Slack app window showed that extensions run there (R10). Sections 1, 5, 7, 9, 11 and 12 updated.

### Evidence register

All sources accessed 2026-10-21. None of them tested Thodu.

- R1. Homerow, product site and user guide. "Click anywhere, without a mouse." Labels on buttons, links and other elements; search that ignores spaces and letter case; Return clicks, Return twice double-clicks, Shift-Return right-clicks, Command-Return opens a link in a new tab; an optional labels-only mode; a scroll mode with h, j, k and l, Shift for larger steps, Tab to cycle scroll areas and digits to jump to one; macOS 13 or later. "Homerow works with most native macOS apps and a growing set of non-native web-based apps." Limits: vendor pages; they do not say how elements are found. https://www.homerow.app/ and https://github.com/nchudleigh/homerow
- R2. neru, README and documentation, release 1.59.0 of 2026-10-19. Hints from the accessibility tree, OCR, shape detection and layout; hint search; scroll, grid, recursive grid and bisect modes; Linux support is beta. Chromium and Electron need `--force-renderer-accessibility`. Wayland global hotkeys need the `input` group, and scrolling needs a writable `/dev/uinput`; without it "Chromium and Electron apps on Hyprland ignore" the compositor's scroll. Limits: the project's own documentation, which records that its Linux claims rest on CI rather than daily use. https://github.com/y3owk1n/neru
- R3. neru issue 1807, "axtree hints find nothing on Hyprland/Wayland: the scan stops before walking any tree", opened 2026-10-14 and open on 2026-10-21. On Hyprland 0.56.2 the tree strategy returns no targets in any app while OCR and shapes work; the reporter traces it to matching the Wayland app id against the AT-SPI application name, and notes that Hyprland supplies the process id. Limits: one reporter's diagnosis; the maintainer pointed to fixes on nightly builds. https://github.com/y3owk1n/neru/issues/1807
- R4. woints, README and docs/toolkits.md, commit 427ad6a, first pushed 2026-10-07. Keyboard hints for Hyprland from AT-SPI, a quickshell module and a vision fallback, verified on Hyprland 0.56.2. Measured per toolkit: GTK3, GTK4 and Qt Widgets expose trees; Chromium and Electron trees wake on a `GetAttributes` call, Chrome from 3 to 240 nodes within a second and from about 512 to 532 MB; Firefox's page is not returned by the collection query; quickshell windows are not reachable over AT-SPI. Vision on a foot window: 54 ms median, 64 ms at the 95th percentile. It detects Lua or legacy dispatch syntax at start. No permissively licensed GUI detector was found; YOLO weights and OmniParser's detector are AGPL. Limits: one developer's measurements on one machine; the project was two weeks old with no outside users. https://github.com/wochap/woints
- R5. hints by Alfredo Sequeida, README, wiki and issues, release 0.1.1 of 2026-06-28. Labels from AT-SPI with an OpenCV fallback; a background service clicks through uinput after a setup step run with sudo; Chromium apps need `ACCESSIBILITY_ENABLED=1` and `--force-renderer-accessibility`. Issue 62, open, reports labels in the wrong place on Hyprland with a custom screen scale; pull request 64, which proposes a fix, is unmerged. The README tells developers how to escape when hints keeps all keyboard input. Limits: issue reports, not reproduced here. https://github.com/AlfredoSequeida/hints
- R6. wl-kbptr, README. Moves the pointer by tile, bisect, split and floating modes, with optional OpenCV region detection, through the wlroots virtual pointer protocol; its Hyprland example uses `hyprctl keyword`. warpd, a grid and hint pointer tool, was last pushed in July 2024. Limits: project documentation. https://github.com/moverest/wl-kbptr and https://github.com/rvaiya/warpd
- R7. Omarchy 4.0.6, shell plugin README at `/usr/share/omarchy/shell/plugins/README.md`. First-party plugins run inside the long-lived omarchy-shell process; user plugins live under `~/.config/omarchy/plugins/<plugin-id>/` with the same manifest contract; overlays load on demand; the menu's path from keybinding through IPC to visible costs about 30 ms cold. Limits: Omarchy's own description of an interface that changes between releases.
- R8. Observations of GG's machine on 2026-10-21, read-only. Hyprland 0.56.2 with the Lua parser, where `hyprctl keyword` is rejected; one display, eDP-1, 1920x1200 at scale 1.25. On the accessibility bus only Evince exposed a window tree; Chromium and its Slack app window were not registered, with no accessibility variable set in the session; Zed and foot were not registered; quickshell was registered with no children. `/dev/uinput` was readable only by root and the user was not in the `input` group. Vimium was installed in Chromium. tesseract 5.5.3 and OpenCV 5.0.0 were installed. Limits: one session's snapshot.
- R9. Fedora Workstation issue 473 and Arch Linux bug FS#75143, on the Steam udev rules. Steam's packaging grants the logged-in user `/dev/uinput` with `TAG+="uaccess"` and `OPTIONS+="static_node=uinput"`; Fedora's maintainers kept the rule after a security discussion. Limits: packaging discussions, not documentation of the mechanism. https://pagure.io/fedora-workstation/issue/473 and https://bugs.archlinux.org/task/75143
- R10. GG's check on 2026-10-21: Vimium's labels work in the Slack app window, a Chromium app window, so extension content scripts run in Omarchy's web apps. Limits: one app, checked by hand.

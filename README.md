# Thodu

Keyboard hints for everything on an Omarchy screen: Homerow for Omarchy. Press a key, labels appear on every button, link, menu item and bar widget in view, and typing a few letters of a target's text, or Shift and its label, clicks it. Thodu is Tamil for "touch".

- [SEED.md](SEED.md): purpose, principles, decisions and evidence.
- [SPEC.md](SPEC.md): the contract the code is built against, with REQ IDs and runnable acceptance.
- [AGENTS.md](AGENTS.md): how agents take and finish work; [tasks/](tasks/) holds the work, from task 0001 (click a button in a GTK3 window) to 0007 (install and doctor).
- `tools/gate`: the gate every change passes. `tools/agent-loop`: runs a build agent in its own worktree. `tools/land`: lands a branch.

Status: set up from the Vithai template; nothing built yet. Requirements: git, Python 3, bubblewrap, rustup, Qt 6 and Omarchy.

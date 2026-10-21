# Tasks

One unit of work per file, `tasks/NNNN-name.md`, numbered in the order to take them. The coordinator writes tasks; an agent claims one with a lock in `tasks/locks/` (AGENTS.md, Taking work). The gate checks every task file's form, and in its full tier re-runs the acceptance of every task in review or done, so a status is checked, not claimed.

```
# NNNN: Title

Status: open
Depends on: none
Spec: REQ-AREA-NN, REQ-AREA-NN
Goal: one paragraph on what this task delivers and why.

Acceptance:
- run: `a command that exits 0 only when the task is done`
- What a reviewer checks by eye, if anything, in one line each.

## Log

- YYYY-MM-DD: created.
```

Status is one of open, claimed, review, blocked or done. An agent sets review or blocked; the coordinator sets done when the work lands. Depends on lists task numbers or none. Spec lists the requirement IDs the task delivers; once it is in review, a test must name each of them.

The Acceptance section is protected: changing it on a task that already exists needs a `Protected: tasks/NNNN-name.md#acceptance: <why>` line in the commit, and the coordinator's acceptance at landing. Write the acceptance command when the task is written, before any code.

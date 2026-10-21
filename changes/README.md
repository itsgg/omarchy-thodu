# Changes to the spec

Agents never edit SPEC.md; they propose a change here as `changes/NNNN-name.md`, and the coordinator merges it. A change that keeps SEED.md's commitments and loosens or removes no check is merged by the coordinator. One that changes a SEED commitment or principle, removes a requirement or loosens a check waits for GG; independent work continues meanwhile. A change that alters no requirement needs no file here.

```
# NNNN: Title

Status: proposed
Why: what made the current spec wrong or incomplete, with the task or evidence.

## Added

### REQ-AREA-NN: Title
The full requirement, with its acceptance.

## Modified

### REQ-AREA-NN: Title
The full new text, replacing the old.

## Removed

- REQ-AREA-NN: why.

## Tests

- Which tests are added, changed or retired with this change.
```

Merging lands in one commit: the SPEC.md edit with a `Protected: SPEC.md: <change number>` line, its tests, an entry in SPEC section 13, and Status set to merged here. IDs are never reused; removed ones move to SPEC's retired list. Rejected changes stay here with Status rejected and the reason.

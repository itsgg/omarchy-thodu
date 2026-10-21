# 0003: Labels-only mode

Status: review
Depends on: 0001
Spec: REQ-SEARCH-02
Goal: the `labels_only` setting, where letters type labels and / starts a search, read from ~/.config/thodu/config.toml.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-SEARCH-02`
- A reviewer checks that the default stays search-first.

## Log

- 2026-10-21: created.
- 2026-10-21: `labels_only = true` in ~/.config/thodu/config.toml makes letters type labels and / start a search; Shift still types a label during a search, and Backspace past an empty search goes back to labels. The file is read at each session, an unknown key or a wrong type is named in the status line and the defaults are used. The default stays search-first: Config::default() has labels_only false, checked by req_search_02_setting and the last two cases of spec/cases/search/labels-only.jsonl. Live on the GTK3 reference window: s focused Save, then /, c, l kept Close alone.


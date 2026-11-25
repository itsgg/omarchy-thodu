# 0031: Install keeps the user's files whole and applies the environment at once

Status: proposed
Why: Task 0039. REQ-INSTALL-01 said what install sets but not what it must leave alone, so the code read `bindings.lua` with a fallback to an empty file (one byte that is not UTF-8 made it rewrite the file with only Thodu's lines), wrote it in place (a full disk truncated it), matched only double-quoted keys on one line, and appended after a final `return`. It said the variables go into "the Hyprland session environment", which install met by editing the file alone: Omarchy copies Hyprland's environment to systemd only at login, so apps started through `uwsm-app` lacked them until the next login while doctor, reading the file, reported them present. The unit could be written with a bare `ExecStart=thodud`. REQ-LABEL-01 promises labels of at most three letters, but targets that join a session late get labels beneath one reserved letter, which holds 676 of that length; the image detector's cap of 500 kept under it by coincidence. REQ-SHELL-02 did not say that disabled menu rows must not act, and the editor action wrote the defaults over a configuration it had failed to read. REQ-ACT-05 did not say what an unreadable settings file does.

## Modified

### REQ-INSTALL-01: Works on Omarchy as installed
When `thodu install` runs, the system shall set `ACCESSIBILITY_ENABLED=1` and `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1` in the Hyprland configuration and in the running user session (the systemd user manager's environment and D-Bus activation's), install a udev rule giving the logged-in user `/dev/uinput` through `uaccess`, install and enable the overlay plugin and the `thodud` user service, and bind keys in the user's Lua configuration; `thodu doctor` shall report each of these as present or missing, a variable present only when both the configuration and the running session have it. On a fresh configuration with no active Thodu bindings, an install without key options shall bind `SUPER + SHIFT + F12` to `thodu hints`, `SUPER + CTRL + F12` to `thodu grid`, and `SUPER + ALT + F12` to `thodu scroll`. Re-running without options shall preserve existing Thodu bindings, their key expressions as written (in any quoting, concatenated or computed), and add the default scroll binding if it is missing. Supplying `--scroll-key` shall set only the scroll binding; supplying both `--hints-key` and `--grid-key` shall replace those bindings, with `--scroll-key` optional. Install shall keep every byte of the bindings file outside the Thodu bindings it replaces, replace a binding written across several lines whole, put what it adds before a top-level `return`, follow a symlinked file to where it lives, and replace the file atomically (a temporary file in the same directory, flushed, then renamed), so a failed write leaves it whole. A bindings file that cannot be read, is not UTF-8 or cannot be read as Lua shall make install exit 1 with an ERROR line before it changes anything. The service shall run `thodud` by its absolute path, quoted for systemd with `%` doubled; when no `thodud` is beside `thodu` or on `PATH`, or its path holds a character systemd refuses in a command (a quote, a backslash, `*`, `?`, `[` or a control character), install shall exit 1 with an ERROR line.
Acceptance:
- scenario: on a fake home directory, `thodu install` then `thodu doctor` reports every item present, and `thodu doctor` alone reports them missing.
- scenario: on a fresh fake home directory, non-interactive `thodu install` writes all three F12 defaults and `thodu doctor` reports the bindings present.
- scenario: on a fake home directory with custom hints and grid bindings, install without key options preserves them and adds the default scroll binding.
- scenario: `--scroll-key` changes only the scroll binding, and supplying all explicit options replaces all three bindings.
- scenario: a bindings file that is not UTF-8 is refused and kept byte for byte; single-quoted, concatenated and computed keys survive a rerun; a binding split across lines is replaced whole; a file ending in `return` gets the bindings before it; each result parses as Lua.
- scenario: under a file size limit the bindings write fails and the file is left whole.
- scenario: install applies the variables to the running session; with them gone from it, doctor reports them missing.
- scenario: the unit names `thodud` by a quoted absolute path through a space, `%` and `$`, and `systemd-analyze verify` finds the command; with no `thodud` to find, install refuses and leaves the unit as it was.

### REQ-LABEL-01: Prefix-free labels from the home row
The system shall assign each target a distinct label of at most three letters from the alphabet `asdfghjkl` then `qwertyuiopzxcvbnm`, none a prefix of another, the shortest labels to the targets nearest the window's top-left. Targets that join a session in a later generation, such as image targets, shall be labelled beneath the reserved prefix, at most as many as fit there within three letters (the alphabet's size squared); past that, the farthest are left out and the status says how many. The image detector's cap shall be checked against that capacity when the daemon is built.
Acceptance:
- property: for any n from 1 to 2000 targets, the labels are distinct, prefix-free and at most three letters (fixed seed).
- cases: `spec/cases/labels/assign.jsonl`.
- scenario: 24 more late targets than the reserved prefix holds are left out and counted; every label stays within three letters and prefix-free, and shown labels are kept.
- scenario: the detector's cap is within the capacity, and a full detector's worth of labels beneath the prefix is within three letters.

### REQ-SHELL-02: The bar widget shows Thodu status and controls
The current text, with these sentences added after "disable direct setting changes while keeping the editor action available": "A disabled row shall do nothing when activated from the pointer or the keyboard. The editor action shall write the defaults only to a configuration it read and found missing or blank, and shall do nothing with one it could not read. A failed write of the configuration shall be shown in the menu."
Acceptance, added:
- scenario: a QML test verifies Enter and arrows on disabled rows send no action while modes stay available, and that the editor never writes over a configuration that was not read.

### REQ-ACT-05: Configure scroll distance
The current text, with its last sentence replaced by: "Invalid values, or a file that exists but cannot be read or is not UTF-8, shall make the configuration report an error that names the key or the problem and use defaults for the session."
Acceptance, added:
- scenario: config tests verify each error names its key, and that an unreadable or non-UTF-8 file is reported with the defaults in use.

## Tests

- crates/thodu/tests/acceptance.rs: req_install_01_bindings_that_are_not_utf8_are_refused_and_kept, req_install_01_rerun_keeps_single_quoted_and_concatenated_keys, req_install_01_a_binding_split_across_lines_is_replaced_whole, req_install_01_a_file_ending_in_return_gets_bindings_before_it, req_install_01_a_failed_bindings_write_leaves_the_file_whole, req_install_01_a_symlinked_bindings_file_stays_a_link, req_install_01_doctor_checks_the_live_session_environment, req_install_01_service_names_thodud_by_a_quoted_absolute_path and req_cli_01_bad_arguments_exit_2_with_an_error_line are added. req_install_01_fake_home_reports_missing_then_installed_reports_present now expects the quoted absolute `ExecStart` and checks the session environment calls; its stubs record `set-environment` and answer `show-environment`.
- crates/thodu/src/main.rs: req_install_01_bindings_plans_keep_what_they_do_not_own, req_install_01_unit_paths_are_quoted_for_systemd and req_install_01_plugin_assets_are_the_tracked_plugin_files are added.
- crates/thodu-core/tests/limits.rs: req_label_01_late_targets_stay_within_three_letters, req_session_07_backspace_focuses_the_area_its_prefix_names, req_session_07_backspace_matches_typing_the_shorter_prefix, req_act_05_settings_errors_name_their_key and req_act_05_an_unreadable_settings_file_is_reported are added.
- crates/thodud/src/image.rs: req_label_01_image_cap_fits_beneath_the_reserved_label_prefix is added.
- tests/qml/tst_barmenu_disabled.qml and tests/qml/tst_reconnect.qml are added.

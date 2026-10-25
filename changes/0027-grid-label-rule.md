# 0027: The grid follows the label rule; bar clicks that switch workspace end the session

Status: merged
Why: Task 0032. After change 0024 a lowercase label clicks and Shift selects, but the grid still needed Return after its last label, and Return on a cell that Tab focused before it was fine clicked the center of a coarse cell. GG also found that clicking a workspace on the bar through the grid switched and switched back: change 0009 had fixed that for bar targets in task 0009 and was never merged, and its check looked only at bar targets.

## Modified

### REQ-GRID-01: Every point is reachable
Choosing a cell refines it the same way until a cell is at most 10 logical units. A label that chooses such a fine cell clicks its center at once when its final character is lowercase, as REQ-ACT-01 describes, and selects it when uppercase, for Return, Shift-Return, Control-Return or a double Return (REQ-ACT-02). Return on a focused cell that is not yet fine refines into it instead of clicking. A scenario covers these.

### REQ-ACT-02: The other clicks and deliberate double Return
Adds change 0009's exception, generalized: if the first click lands on the bar, through a bar target or a grid cell over it, and changes the monitor's active workspace, the overlay ends the session after the click and does not take the keyboard back, which would switch the workspace back.

## Tests

- crates/thodu-core/tests/req.rs: req_grid_01_a_lowercase_label_on_a_fine_cell_clicks and req_grid_01_return_on_a_coarse_cell_refines are added; req_grid_01_every_point_is_reachable and req_session_01_fine_grid_cell_is_not_a_label_selected_target type the last letter with Shift to select the fine cell.
- crates/thodud/src/main.rs: req_act_02_clicks_on_the_bar_watch_for_a_workspace_switch is added; task 0009's tests cover the overlay ending on a workspace switch.

//! Summoning the overlay plugin inside omarchy-shell (SPEC section 4).

use std::path::Path;
use std::process::Command;

pub const PLUGIN_ID: &str = "io.github.itsgg.thodu";

/// Ask omarchy-shell to open the overlay on `session`; `test` keeps it from
/// taking the keyboard, for live checks that send keys over the IPC.
pub fn summon(session: &str, socket: &Path, test: bool) -> Result<(), String> {
    let payload =
        serde_json::json!({ "session": session, "socket": socket, "test": test }).to_string();
    let out = Command::new("omarchy-shell")
        .args(["shell", "summon", PLUGIN_ID, &payload])
        .output()
        .map_err(|e| format!("cannot run omarchy-shell: {e}"))?;
    let said = String::from_utf8_lossy(&out.stdout);
    if out.status.success() && said.trim() == "ok" {
        Ok(())
    } else {
        Err(format!(
            "omarchy-shell did not open the overlay ({}); is the plugin installed and enabled? make install, then omarchy plugin enable {PLUGIN_ID}",
            said.trim()
        ))
    }
}

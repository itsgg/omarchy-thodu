//! What an application that is not on the accessibility bus needs to be
//! there (REQ-SESSION-05), judged from its executable and the libraries it
//! has loaded, since an unregistered app cannot say what it is.

/// The toolkit a process uses, from its executable's name and its loaded
/// libraries' file names.
pub fn toolkit(exe: &str, libs: &[String]) -> &'static str {
    let exe = exe.rsplit('/').next().unwrap_or(exe).to_lowercase();
    let has = |needle: &str| libs.iter().any(|l| l.contains(needle));
    if exe.starts_with("electron") || has("libnode.so") {
        "Electron"
    } else if exe.contains("chrom")
        || exe.contains("brave")
        || exe.contains("vivaldi")
        || has("libcef")
    {
        "Chromium"
    } else if has("libQt6Gui") || has("libQt5Gui") {
        "Qt"
    } else if has("libgtk-4.so") {
        "GTK4"
    } else if has("libgtk-3.so") {
        "GTK3"
    } else {
        ""
    }
}

/// The status line for a window whose process is not on the bus.
pub fn message(class: &str, exe: &str, libs: &[String]) -> String {
    let needs = match toolkit(exe, libs) {
        "Chromium" | "Electron" => {
            "it starts its accessibility tree only with ACCESSIBILITY_ENABLED=1 in its environment; thodu install sets it, then restart the app"
        }
        "Qt" => {
            "Qt registers only with QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 in its environment; thodu install sets it, then restart the app"
        }
        "GTK3" | "GTK4" => {
            "GTK registers unless NO_AT_BRIDGE=1 or GTK_A11Y=none is in its environment; remove it and restart the app"
        }
        _ => "it does not use the accessibility bus; Space opens the grid",
    };
    format!("{class} is not on the accessibility bus: {needs}")
}

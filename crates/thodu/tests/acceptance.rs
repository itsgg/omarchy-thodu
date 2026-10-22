use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestRoot(PathBuf);

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl TestRoot {
    fn new(name: &str) -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").expect("HOME is set"));
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = home
            .join(".local/share/thodu/tests")
            .join(format!("{name}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create test data directory");
        let bin = path.join("bin");
        std::fs::create_dir_all(&bin).expect("create stub command directory");
        write_executable(
            &bin.join("omarchy-shell"),
            r##"#!/bin/sh
set -eu
mkdir -p "$HOME/.local/share/thodu/tests"
printf 'omarchy-shell %s\n' "$*" >> "$HOME/.local/share/thodu/tests/omarchy.log"
[ "$1" = shell ] && [ "$2" = rescanPlugins ] || exit 2
printf 'ok\n'
"##,
        );
        write_executable(
            &bin.join("omarchy"),
            r##"#!/bin/sh
set -eu
mkdir -p "$HOME/.local/share/thodu/tests"
printf 'omarchy %s\n' "$*" >> "$HOME/.local/share/thodu/tests/omarchy.log"
if [ "$1" = plugin ] && [ "$2" = list ] && [ "$3" = --json ]; then
  printf '[{"id":"io.github.itsgg.thodu","kinds":["overlay","bar-widget"],"enabled":false}]\n'
  exit 0
fi
if [ "$1" = plugin ] && [ "$2" = enable ] && [ "$3" = io.github.itsgg.thodu ]; then
  python3 - "$HOME/.config/omarchy/shell.json" "$3" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
config = json.loads(path.read_text()) if path.exists() else {}
layout = config.setdefault("bar", {}).setdefault("layout", {})
right = layout.setdefault("right", [])
if not any(item.get("id") == sys.argv[2] for item in right):
    right.append({"id": sys.argv[2]})
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(config) + "\n")
PY
  exit 0
fi
exit 2
"##,
        );
        Self(path)
    }

    fn home(&self) -> PathBuf {
        self.0.join("home")
    }

    fn udev_dir(&self) -> PathBuf {
        self.0.join("udev")
    }
}

fn write_executable(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write stub command");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("make stub command executable");
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn thodu(home: &Path, udev_dir: &Path, args: &[&str], cwd: &Path) -> Output {
    let mut search_path = vec![cwd.parent().expect("test root").join("bin")];
    if let Some(path) = std::env::var_os("PATH") {
        search_path.extend(std::env::split_paths(&path));
    }
    let search_path = std::env::join_paths(search_path).expect("build PATH");
    Command::new(env!("CARGO_BIN_EXE_thodu"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("PATH", search_path)
        .env("THODU_UDEV_DIR", udev_dir)
        .env("ACCESSIBILITY_ENABLED", "1")
        .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1")
        .output()
        .expect("run thodu")
}

// REQ-INSTALL-01: thodu install writes every required file and doctor reports
// each setting from the fake home, even when the caller environment exports
// accessibility variables.
#[test]
fn req_install_01_fake_home_reports_missing_then_installed_reports_present() {
    let root = TestRoot::new("install");
    let home = root.home();
    let udev_dir = root.udev_dir();
    let cwd = root.0.join("elsewhere");
    std::fs::create_dir_all(&cwd).expect("create unrelated working directory");
    let bindings = home.join(".config/hypr/bindings.lua");
    std::fs::create_dir_all(bindings.parent().expect("bindings parent"))
        .expect("create Hyprland config directory");
    std::fs::write(
        &bindings,
        "-- thodu hints and thodu grid are mentioned here, but are not bindings\n",
    )
    .expect("write commented binding names");
    let shell_json = home.join(".config/omarchy/shell.json");
    std::fs::create_dir_all(shell_json.parent().expect("shell config parent"))
        .expect("create Omarchy config directory");
    std::fs::write(
        &shell_json,
        r#"{"bar":{"layout":{"right":[{"id":"omarchy.audio"}]}}}"#,
    )
    .expect("write initial Omarchy config");

    let before = thodu(&home, &udev_dir, &["doctor"], &cwd);
    assert_eq!(before.status.code(), Some(1));
    let before = String::from_utf8_lossy(&before.stdout);
    assert!(
        before.contains("ACCESSIBILITY_ENABLED=1: missing"),
        "{before}"
    );
    assert!(
        before.contains("QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1: missing"),
        "{before}"
    );

    let installed = thodu(&home, &udev_dir, &["install"], &cwd);
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );

    let rule = std::fs::read_to_string(udev_dir.join("99-thodu-uinput.rules"))
        .expect("udev rule installed in the configured rules directory");
    assert!(rule.contains("TAG+=\"uaccess\""), "{rule}");
    assert!(
        !home
            .join(".config/udev/rules.d/99-thodu-uinput.rules")
            .exists(),
        "udev rule must not be installed in a home config directory"
    );

    for path in [
        ".config/omarchy/plugins/io.github.itsgg.thodu/manifest.json",
        ".config/omarchy/plugins/io.github.itsgg.thodu/Thodu.qml",
        ".config/omarchy/plugins/io.github.itsgg.thodu/BarProbe.qml",
        ".config/omarchy/plugins/io.github.itsgg.thodu/components/Client.qml",
        ".config/omarchy/plugins/io.github.itsgg.thodu/components/Listener.qml",
        ".config/omarchy/plugins/io.github.itsgg.thodu/components/SessionView.qml",
        ".config/omarchy/plugins/io.github.itsgg.thodu/lib/BarScan.js",
    ] {
        assert!(home.join(path).is_file(), "missing plugin asset {path}");
    }
    let adjacent_daemon = PathBuf::from(env!("CARGO_BIN_EXE_thodu")).with_file_name("thodud");
    let expected_daemon = if adjacent_daemon.exists() {
        adjacent_daemon
    } else {
        PathBuf::from("thodud")
    };
    let service = std::fs::read_to_string(home.join(".config/systemd/user/thodud.service"))
        .expect("read daemon service");
    assert!(
        service.contains(&format!("ExecStart={}", expected_daemon.display())),
        "{service}"
    );
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&shell_json).expect("read enabled Omarchy config"))
            .expect("parse enabled Omarchy config");
    let right = config["bar"]["layout"]["right"]
        .as_array()
        .expect("right bar layout");
    assert!(right.iter().any(|item| item["id"] == "omarchy.audio"));
    assert!(
        right
            .iter()
            .any(|item| item["id"] == "io.github.itsgg.thodu")
    );
    let omarchy_log = std::fs::read_to_string(home.join(".local/share/thodu/tests/omarchy.log"))
        .expect("read Omarchy command log");
    assert!(
        omarchy_log.contains("omarchy-shell shell rescanPlugins"),
        "{omarchy_log}"
    );
    assert!(
        omarchy_log.contains("omarchy plugin enable io.github.itsgg.thodu --section right"),
        "{omarchy_log}"
    );

    let after = thodu(&home, &udev_dir, &["doctor"], &cwd);
    assert!(
        after.status.success(),
        "stdout: {}; stderr: {}",
        String::from_utf8_lossy(&after.stdout),
        String::from_utf8_lossy(&after.stderr)
    );
    assert!(
        String::from_utf8_lossy(&after.stdout)
            .lines()
            .all(|line| line.ends_with(": present")),
        "{}",
        String::from_utf8_lossy(&after.stdout)
    );
    let installed_bindings = std::fs::read_to_string(&bindings).expect("read installed bindings");
    assert!(
        installed_bindings
            .lines()
            .any(|line| !line.trim_start().starts_with("--") && line.contains("thodu hints")),
        "{installed_bindings}"
    );
    assert!(
        installed_bindings
            .lines()
            .any(|line| !line.trim_start().starts_with("--") && line.contains("thodu grid")),
        "{installed_bindings}"
    );
    assert!(
        installed_bindings.contains("SUPER + SHIFT + J")
            && installed_bindings.contains("SUPER + CTRL + J"),
        "{installed_bindings}"
    );

    std::fs::remove_file(
        home.join(".config/omarchy/plugins/io.github.itsgg.thodu/components/Client.qml"),
    )
    .expect("remove a plugin asset");
    let incomplete = thodu(&home, &udev_dir, &["doctor"], &cwd);
    assert_eq!(incomplete.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&incomplete.stdout).contains("overlay plugin: missing"),
        "{}",
        String::from_utf8_lossy(&incomplete.stdout)
    );
}

// REQ-INSTALL-01: install repairs an existing dangling service enable link.
#[test]
fn req_install_01_install_repairs_a_dangling_service_link() {
    let root = TestRoot::new("broken-service-link");
    let home = root.home();
    let udev_dir = root.udev_dir();
    let cwd = root.0.join("elsewhere");
    std::fs::create_dir_all(&cwd).expect("create unrelated working directory");

    let wants = home.join(".config/systemd/user/default.target.wants");
    std::fs::create_dir_all(&wants).expect("create wants directory");
    std::os::unix::fs::symlink("../missing.service", wants.join("thodud.service"))
        .expect("create dangling link");

    let installed = thodu(&home, &udev_dir, &["install"], &cwd);
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    assert_eq!(
        std::fs::read_link(wants.join("thodud.service")).expect("read service link"),
        PathBuf::from("../thodud.service")
    );
}

// REQ-CLI-02: thodu hints rejects a stale Hyprland socket at the command line.
#[test]
fn req_cli_02_no_hyprland_socket_exits_1_with_error() {
    let root = TestRoot::new("stale-hyprland-socket");
    let home = root.home();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let runtime = root
        .0
        .parent()
        .expect("test data parent")
        .join(format!("rt-{}-{stamp}", std::process::id()));
    let _cleanup = RemoveOnDrop(runtime.clone());
    let signature = "s";
    let hypr_dir = runtime.join("hypr").join(signature);
    let socket = hypr_dir.join(".socket.sock");
    std::fs::create_dir_all(socket.parent().expect("socket parent"))
        .expect("create fake Hyprland runtime");
    drop(UnixListener::bind(&socket).expect("bind temporary Hyprland socket"));
    assert!(
        socket.exists(),
        "dropped Unix listener leaves a stale socket"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_thodu"))
        .args(["hints", "--no-overlay"])
        .current_dir(&root.0)
        .env("HOME", &home)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("HYPRLAND_INSTANCE_SIGNATURE", signature)
        .output()
        .expect("run thodu hints");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "unexpected stdout: {:?}",
        output.stdout
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("ERROR "), "{stderr}");
    assert!(stderr.contains("cannot connect to Hyprland"), "{stderr}");
}

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
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
        let uinput = path.join("uinput");
        std::fs::write(&uinput, []).expect("create fake uinput node");
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
        write_executable(
            &bin.join("systemctl"),
            r##"#!/bin/sh
set -eu
mkdir -p "$HOME/.local/share/thodu/tests"
printf 'systemctl %s\n' "$*" >> "$HOME/.local/share/thodu/tests/systemctl.log"
env_file="$HOME/.local/share/thodu/tests/session-environment"
if [ "$2" = set-environment ]; then
  shift 2
  for assignment in "$@"; do
    printf '%s\n' "$assignment" >> "$env_file"
  done
  exit 0
fi
if [ "$2" = show-environment ]; then
  [ -f "$env_file" ] && cat "$env_file"
  exit 0
fi
if [ "$2" = is-active ]; then
  if [ "$3" = thodud.service ] && [ -e "$HOME/.local/share/thodu/tests/service-inactive" ]; then
    printf 'inactive\n'
    exit 3
  fi
  printf 'active\n'
fi
"##,
        );
        write_executable(
            &bin.join("dbus-update-activation-environment"),
            r##"#!/bin/sh
set -eu
mkdir -p "$HOME/.local/share/thodu/tests"
printf 'dbus-update-activation-environment %s\n' "$*" >> "$HOME/.local/share/thodu/tests/dbus.log"
"##,
        );
        // A thodud on PATH for the service unit when none sits beside thodu.
        write_executable(&bin.join("thodud"), "#!/bin/sh\nexit 0\n");
        write_executable(
            &bin.join("udevadm"),
            r##"#!/bin/sh
set -eu
mkdir -p "$HOME/.local/share/thodu/tests"
printf 'udevadm %s\n' "$*" >> "$HOME/.local/share/thodu/tests/udevadm.log"
"##,
        );
        write_executable(
            &bin.join("modprobe"),
            r##"#!/bin/sh
set -eu
mkdir -p "$HOME/.local/share/thodu/tests"
printf 'modprobe %s\n' "$*" >> "$HOME/.local/share/thodu/tests/modprobe.log"
"##,
        );
        write_executable(
            &bin.join("hyprctl"),
            r##"#!/bin/sh
set -eu
mkdir -p "$HOME/.local/share/thodu/tests"
printf 'hyprctl %s\n' "$*" >> "$HOME/.local/share/thodu/tests/hyprctl.log"
[ "$1" = reload ]
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

    fn runtime_dir(&self) -> PathBuf {
        self.0.join("runtime")
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
    thodu_command(home, udev_dir, args, cwd)
        .output()
        .expect("run thodu")
}

fn thodu_command(home: &Path, udev_dir: &Path, args: &[&str], cwd: &Path) -> Command {
    let mut search_path = vec![cwd.parent().expect("test root").join("bin")];
    if let Some(path) = std::env::var_os("PATH") {
        search_path.extend(std::env::split_paths(&path));
    }
    let search_path = std::env::join_paths(search_path).expect("build PATH");
    let mut command = Command::new(env!("CARGO_BIN_EXE_thodu"));
    command
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("PATH", search_path)
        .env("THODU_UDEV_DIR", udev_dir)
        .env(
            "THODU_UDEVADM",
            cwd.parent().expect("test root").join("bin/udevadm"),
        )
        .env(
            "THODU_MODPROBE",
            cwd.parent().expect("test root").join("bin/modprobe"),
        )
        .env(
            "THODU_UINPUT_DEVICE",
            cwd.parent().expect("test root").join("uinput"),
        )
        .env(
            "THODU_SYSTEMCTL",
            cwd.parent().expect("test root").join("bin/systemctl"),
        )
        .env(
            "THODU_HYPRCTL",
            cwd.parent().expect("test root").join("bin/hyprctl"),
        )
        .env(
            "THODU_DBUS_UPDATE_ENV",
            cwd.parent()
                .expect("test root")
                .join("bin/dbus-update-activation-environment"),
        )
        .env(
            "XDG_RUNTIME_DIR",
            cwd.parent().expect("test root").join("runtime"),
        )
        .env("ACCESSIBILITY_ENABLED", "1")
        .env("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "1");
    command
}

fn plugin_asset_paths() -> Vec<PathBuf> {
    fn visit(path: &Path, root: &Path, files: &mut Vec<PathBuf>) {
        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(path)
                .expect("read plugin asset directory")
                .map(|entry| entry.expect("read plugin asset").path())
                .collect();
            entries.sort();
            for entry in entries {
                visit(&entry, root, files);
            }
        } else {
            files.push(
                path.strip_prefix(root)
                    .expect("asset below root")
                    .to_path_buf(),
            );
        }
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for item in include_str!("../../../plugin-files.txt")
        .lines()
        .map(str::trim)
        .filter(|item| !item.is_empty() && !item.starts_with('#'))
    {
        visit(&root.join(item), &root, &mut files);
    }
    files.sort();
    files
}

fn install_args() -> [&'static str; 1] {
    ["install"]
}

fn alternate_install_args() -> [&'static str; 7] {
    [
        "install",
        "--hints-key",
        "SUPER + SHIFT + code:201",
        "--grid-key",
        "SUPER + F10",
        "--scroll-key",
        "CTRL + ALT + SPACE",
    ]
}

fn fake_running_daemon(root: &TestRoot) -> thread::JoinHandle<()> {
    let runtime = root.runtime_dir();
    std::fs::create_dir_all(&runtime).expect("create fake runtime directory");
    let listener =
        UnixListener::bind(runtime.join("thodu.sock")).expect("start fake existing thodud");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept daemon handoff");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("set daemon handoff timeout");
        let mut line = String::new();
        let mut reader = BufReader::new(stream.try_clone().expect("clone daemon stream"));
        reader
            .read_line(&mut line)
            .expect("read daemon handoff request");
        let request: serde_json::Value = serde_json::from_str(&line).expect("parse request");
        assert_eq!(request["op"], "shutdown");
        use std::io::Write;
        stream
            .write_all(b"{\"v\":1,\"ok\":true}\n")
            .expect("reply to handoff");
    })
}

// REQ-INSTALL-01: a fresh non-interactive install uses the project defaults.
#[test]
fn req_install_01_noninteractive_install_uses_f12_defaults() {
    let root = TestRoot::new("default-keys");
    let home = root.home();
    let udev_dir = root.udev_dir();
    let cwd = root.0.join("elsewhere");
    std::fs::create_dir_all(&cwd).expect("create unrelated working directory");

    let output = thodu(&home, &udev_dir, &["install"], &cwd);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bindings = std::fs::read_to_string(home.join(".config/hypr/bindings.lua"))
        .expect("read default keybindings");
    assert!(
        bindings.contains("o.bind(\"SUPER + SHIFT + F12\", \"Thodu hints\"")
            && bindings.contains("o.bind(\"SUPER + CTRL + F12\", \"Thodu grid\"")
            && bindings.contains("o.bind(\"SUPER + ALT + F12\", \"Thodu scroll\""),
        "{bindings}"
    );
    assert!(
        !bindings.contains("CTRL + SPACE") && !bindings.contains("CTRL + SHIFT + SPACE"),
        "a fresh install uses the project defaults: {bindings}"
    );
    let doctor = thodu(&home, &udev_dir, &["doctor"], &cwd);
    assert!(
        doctor.status.success(),
        "stdout: {}; stderr: {}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
}

// REQ-INSTALL-01: an install without key options preserves a local choice.
#[test]
fn req_install_01_noninteractive_install_preserves_existing_bindings() {
    let root = TestRoot::new("preserved-keys");
    let home = root.home();
    let udev_dir = root.udev_dir();
    let cwd = root.0.join("elsewhere");
    std::fs::create_dir_all(&cwd).expect("create unrelated working directory");
    let bindings = home.join(".config/hypr/bindings.lua");
    std::fs::create_dir_all(bindings.parent().expect("bindings parent"))
        .expect("create Hyprland config directory");
    std::fs::write(
        &bindings,
        concat!(
            "o.bind(\"CTRL + SPACE\", \"Thodu hints\", \"thodu hints\")\n",
            "o.bind(\"CTRL + SHIFT + SPACE\", \"Thodu grid\", \"thodu grid\")\n"
        ),
    )
    .expect("write local Thodu bindings");

    let installed = thodu(&home, &udev_dir, &["install"], &cwd);
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let installed_bindings = std::fs::read_to_string(&bindings).expect("read bindings");
    assert!(
        installed_bindings.contains("o.bind(\"CTRL + SPACE\", \"Thodu hints\"")
            && installed_bindings.contains("o.bind(\"CTRL + SHIFT + SPACE\", \"Thodu grid\"")
            && installed_bindings.contains("o.bind(\"SUPER + ALT + F12\", \"Thodu scroll\""),
        "{installed_bindings}"
    );
    assert!(
        !installed_bindings.contains("SUPER + SHIFT + F12")
            && !installed_bindings.contains("SUPER + CTRL + F12"),
        "the no-option install must preserve both local choices: {installed_bindings}"
    );
}

// REQ-INSTALL-01: explicit key options override the project defaults.
#[test]
fn req_install_01_explicit_key_options_override_defaults() {
    let root = TestRoot::new("override-default-keys");
    let home = root.home();
    let udev_dir = root.udev_dir();
    let cwd = root.0.join("elsewhere");
    std::fs::create_dir_all(&cwd).expect("create unrelated working directory");
    let bindings = home.join(".config/hypr/bindings.lua");
    std::fs::create_dir_all(bindings.parent().expect("bindings parent"))
        .expect("create Hyprland config directory");
    std::fs::write(
        &bindings,
        concat!(
            "o.bind(\"SUPER + SHIFT + F12\", \"Thodu hints\", \"thodu hints\")\n",
            "o.bind(\"SUPER + CTRL + F12\", \"Thodu grid\", \"thodu grid\")\n"
        ),
    )
    .expect("write default Thodu bindings");

    let changed = thodu(&home, &udev_dir, &alternate_install_args(), &cwd);
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    let changed_bindings = std::fs::read_to_string(&bindings).expect("read changed bindings");
    assert!(
        changed_bindings.contains("SUPER + SHIFT + code:201")
            && changed_bindings.contains("SUPER + F10")
            && changed_bindings.contains("CTRL + ALT + SPACE"),
        "{changed_bindings}"
    );
    assert!(
        !changed_bindings.contains("SUPER + SHIFT + F12")
            && !changed_bindings.contains("SUPER + CTRL + F12"),
        "explicit options must replace both defaults: {changed_bindings}"
    );
}

// REQ-INSTALL-01: a scroll override leaves the user's hints and grid keys intact.
#[test]
fn req_install_01_scroll_key_override_preserves_other_keys() {
    let root = TestRoot::new("scroll-key-only");
    let home = root.home();
    let udev_dir = root.udev_dir();
    let cwd = root.0.join("elsewhere");
    std::fs::create_dir_all(&cwd).expect("create unrelated working directory");
    let bindings = home.join(".config/hypr/bindings.lua");
    std::fs::create_dir_all(bindings.parent().expect("bindings parent"))
        .expect("create Hyprland config directory");
    std::fs::write(
        &bindings,
        concat!(
            "o.bind(\"CTRL + SPACE\", \"Thodu hints\", \"thodu hints\")\n",
            "o.bind(\"CTRL + SHIFT + SPACE\", \"Thodu grid\", \"thodu grid\")\n"
        ),
    )
    .expect("write local Thodu bindings");

    let installed = thodu(
        &home,
        &udev_dir,
        &["install", "--scroll-key", "CTRL + ALT + SPACE"],
        &cwd,
    );
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let installed_bindings = std::fs::read_to_string(&bindings).expect("read bindings");
    assert!(
        installed_bindings.contains("o.bind(\"CTRL + SPACE\", \"Thodu hints\"")
            && installed_bindings.contains("o.bind(\"CTRL + SHIFT + SPACE\", \"Thodu grid\"")
            && installed_bindings.contains("o.bind(\"CTRL + ALT + SPACE\", \"Thodu scroll\""),
        "{installed_bindings}"
    );
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
    let before_items: Vec<_> = before.lines().collect();
    assert_eq!(before_items.len(), 6, "{before}");
    assert!(
        before_items.iter().all(|line| line.ends_with(": missing")),
        "{before}"
    );
    assert!(
        before.contains("ACCESSIBILITY_ENABLED=1 in Hyprland: missing"),
        "{before}"
    );
    assert!(
        before.contains("QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 in Hyprland: missing"),
        "{before}"
    );

    let handoff = fake_running_daemon(&root);
    let installed = thodu(&home, &udev_dir, &install_args(), &cwd);
    handoff.join().expect("complete fake daemon handoff");
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );

    let rule = std::fs::read_to_string(udev_dir.join("72-thodu-uinput.rules"))
        .expect("udev rule installed in the configured rules directory");
    assert!(rule.contains("TAG+=\"uaccess\""), "{rule}");
    assert!(
        !home
            .join(".config/udev/rules.d/99-thodu-uinput.rules")
            .exists(),
        "udev rule must not be installed in a home config directory"
    );

    for path in plugin_asset_paths() {
        assert!(
            home.join(".config/omarchy/plugins/io.github.itsgg.thodu")
                .join(&path)
                .is_file(),
            "missing plugin asset {}",
            path.display()
        );
    }
    let adjacent_daemon = PathBuf::from(env!("CARGO_BIN_EXE_thodu")).with_file_name("thodud");
    let expected_daemon = if adjacent_daemon.exists() {
        adjacent_daemon
    } else {
        root.0.join("bin/thodud")
    };
    let service = std::fs::read_to_string(home.join(".config/systemd/user/thodud.service"))
        .expect("read daemon service");
    assert!(
        service.contains(&format!("ExecStart=\"{}\"\n", expected_daemon.display())),
        "{service}"
    );
    assert!(
        service.contains("PartOf=graphical-session.target"),
        "{service}"
    );
    assert!(
        service.contains("After=graphical-session.target"),
        "{service}"
    );
    assert!(
        service.contains("WantedBy=graphical-session.target"),
        "{service}"
    );
    assert!(
        home.join(".config/systemd/user/graphical-session.target.wants/thodud.service")
            .is_symlink(),
        "service enabled for the graphical session"
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
        installed_bindings.contains("hl.env(\"ACCESSIBILITY_ENABLED\", \"1\")")
            && installed_bindings.contains("hl.env(\"QT_LINUX_ACCESSIBILITY_ALWAYS_ON\", \"1\")"),
        "{installed_bindings}"
    );
    assert!(
        installed_bindings.contains("SUPER + SHIFT + F12"),
        "{installed_bindings}"
    );
    assert!(
        installed_bindings.contains("SUPER + CTRL + F12"),
        "{installed_bindings}"
    );
    assert!(
        installed_bindings.contains("SUPER + ALT + F12"),
        "{installed_bindings}"
    );
    assert!(
        installed_bindings.contains(&env!("CARGO_BIN_EXE_thodu").replace('\\', "\\\\")),
        "keybindings launch the installed command by path: {installed_bindings}"
    );
    let udev_log = std::fs::read_to_string(home.join(".local/share/thodu/tests/udevadm.log"))
        .expect("read udevadm log");
    assert!(udev_log.contains("control --reload-rules"), "{udev_log}");
    assert!(
        udev_log.contains("trigger --action=change --subsystem-match=misc --sysname-match=uinput"),
        "{udev_log}"
    );
    assert!(udev_log.contains("udevadm settle"), "{udev_log}");
    let modprobe_log = std::fs::read_to_string(home.join(".local/share/thodu/tests/modprobe.log"))
        .expect("read modprobe log");
    assert_eq!(modprobe_log.lines().count(), 1, "{modprobe_log}");
    assert!(modprobe_log.contains("modprobe uinput"), "{modprobe_log}");
    let systemctl_log =
        std::fs::read_to_string(home.join(".local/share/thodu/tests/systemctl.log"))
            .expect("read systemctl log");
    assert!(
        systemctl_log.contains("--user daemon-reload"),
        "{systemctl_log}"
    );
    assert!(
        systemctl_log.contains("--user enable thodud.service"),
        "{systemctl_log}"
    );
    assert!(
        systemctl_log.contains("--user stop thodud.service"),
        "{systemctl_log}"
    );
    assert!(
        systemctl_log.contains("--user start thodud.service"),
        "{systemctl_log}"
    );
    let hyprctl_log = std::fs::read_to_string(home.join(".local/share/thodu/tests/hyprctl.log"))
        .expect("read Hyprland command log");
    assert!(hyprctl_log.contains("hyprctl reload"), "{hyprctl_log}");
    assert!(
        systemctl_log.contains(
            "--user set-environment ACCESSIBILITY_ENABLED=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1"
        ),
        "install applies the variables to the running session: {systemctl_log}"
    );
    let dbus_log = std::fs::read_to_string(home.join(".local/share/thodu/tests/dbus.log"))
        .expect("read D-Bus environment log");
    assert!(
        dbus_log.contains("--systemd ACCESSIBILITY_ENABLED=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1"),
        "{dbus_log}"
    );

    std::fs::write(
        home.join(".local/share/thodu/tests/service-inactive"),
        "inactive",
    )
    .expect("mark the fake service inactive");
    let failed_start = thodu(&home, &udev_dir, &install_args(), &cwd);
    assert_eq!(failed_start.status.code(), Some(1));
    let failed_start_stderr = String::from_utf8_lossy(&failed_start.stderr);
    assert!(
        failed_start_stderr.starts_with("ERROR "),
        "{failed_start_stderr}"
    );
    assert!(
        failed_start_stderr.contains("did not stay active after start"),
        "{failed_start_stderr}"
    );
    let inactive = thodu(&home, &udev_dir, &["doctor"], &cwd);
    assert_eq!(inactive.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&inactive.stdout).contains("thodud user service: missing"),
        "{}",
        String::from_utf8_lossy(&inactive.stdout)
    );
    std::fs::remove_file(home.join(".local/share/thodu/tests/service-inactive"))
        .expect("restore the fake service state");

    let plugin_dir = home.join(".config/omarchy/plugins/io.github.itsgg.thodu");
    let stale_asset = plugin_dir.join("components/Old.qml");
    std::fs::write(&stale_asset, "stale").expect("add a stale installed asset");
    let udev_calls_before_rerun = udev_log.lines().count();
    let rerun = thodu(&home, &udev_dir, &install_args(), &cwd);
    assert!(
        rerun.status.success(),
        "{}",
        String::from_utf8_lossy(&rerun.stderr)
    );
    assert!(
        !stale_asset.exists(),
        "install removes assets absent from the manifest"
    );
    let udev_calls_after_rerun =
        std::fs::read_to_string(home.join(".local/share/thodu/tests/udevadm.log"))
            .expect("read repeated udevadm log")
            .lines()
            .count();
    assert_eq!(udev_calls_after_rerun, udev_calls_before_rerun);
    let omarchy_log_after_rerun =
        std::fs::read_to_string(home.join(".local/share/thodu/tests/omarchy.log"))
            .expect("read repeated Omarchy log");
    assert_eq!(
        omarchy_log_after_rerun
            .lines()
            .filter(|line| line.contains("plugin enable io.github.itsgg.thodu"))
            .count(),
        1,
        "reinstall leaves an already enabled plugin in place"
    );

    let changed_keys = thodu(&home, &udev_dir, &alternate_install_args(), &cwd);
    assert!(
        changed_keys.status.success(),
        "{}",
        String::from_utf8_lossy(&changed_keys.stderr)
    );
    let changed_bindings = std::fs::read_to_string(&bindings).expect("read changed keybindings");
    assert!(
        changed_bindings.contains("SUPER + SHIFT + code:201"),
        "{changed_bindings}"
    );
    assert!(
        changed_bindings.contains("SUPER + F10"),
        "{changed_bindings}"
    );
    assert!(
        !changed_bindings.contains("SUPER + SHIFT + F12"),
        "{changed_bindings}"
    );
    assert!(
        !changed_bindings.contains("SUPER + CTRL + F12"),
        "{changed_bindings}"
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

    let wants = home.join(".config/systemd/user/graphical-session.target.wants");
    std::fs::create_dir_all(&wants).expect("create wants directory");
    std::os::unix::fs::symlink("../missing.service", wants.join("thodud.service"))
        .expect("create dangling link");

    let installed = thodu(&home, &udev_dir, &install_args(), &cwd);
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
    // A stale entry nothing listens on, as a crashed Hyprland leaves. A bound
    // and dropped listener would race: a child spawned by a parallel test
    // holds a copy of it until exec, so a connect could still succeed.
    std::fs::write(&socket, b"").expect("write stale Hyprland socket entry");
    assert!(socket.exists(), "the stale socket entry exists");

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
    assert!(
        stderr.contains("start or return to Hyprland and retry"),
        "{stderr}"
    );
}

// REQ-INSTALL-01: failed uinput access never records installation as applied,
// and a later install retries the permission setup.
#[test]
fn req_install_01_uinput_access_failure_does_not_block_retry() {
    let root = TestRoot::new("uinput-retry");
    let home = root.home();
    let udev_dir = root.udev_dir();
    let cwd = root.0.join("elsewhere");
    std::fs::create_dir_all(&cwd).expect("create unrelated working directory");
    let uinput = root.0.join("uinput");
    std::fs::remove_file(&uinput).expect("remove accessible fake uinput device");

    let failed = thodu(&home, &udev_dir, &install_args(), &cwd);
    assert_eq!(failed.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(stderr.starts_with("ERROR "), "{stderr}");
    assert!(
        stderr.contains("/dev/uinput is still unavailable"),
        "{stderr}"
    );
    assert!(
        !home
            .join(".local/share/thodu/install/udev-applied")
            .exists(),
        "failed access must not write the applied marker"
    );

    std::fs::write(&uinput, []).expect("restore accessible fake uinput device");
    let retried = thodu(&home, &udev_dir, &install_args(), &cwd);
    assert!(
        retried.status.success(),
        "stdout: {}; stderr: {}",
        String::from_utf8_lossy(&retried.stdout),
        String::from_utf8_lossy(&retried.stderr)
    );
    assert!(
        home.join(".local/share/thodu/install/udev-applied")
            .is_file(),
        "successful access records the applied marker"
    );
}

/// A fake home with `bindings.lua` holding `contents`, ready to install.
fn root_with_bindings(name: &str, contents: &[u8]) -> (TestRoot, PathBuf) {
    let root = TestRoot::new(name);
    let bindings = root.home().join(".config/hypr/bindings.lua");
    std::fs::create_dir_all(bindings.parent().expect("bindings parent"))
        .expect("create Hyprland config directory");
    std::fs::write(&bindings, contents).expect("write bindings");
    std::fs::create_dir_all(root.0.join("elsewhere")).expect("create working directory");
    (root, bindings)
}

fn install_in(root: &TestRoot, args: &[&str]) -> Output {
    thodu(
        &root.home(),
        &root.udev_dir(),
        args,
        &root.0.join("elsewhere"),
    )
}

/// REQ-INSTALL-01: the file still parses as Lua. luac checks it where it is
/// installed (it is on GG's machine); elsewhere thodu's own reader did when
/// install read the file back.
fn assert_valid_lua(path: &Path) {
    match Command::new("luac").arg("-p").arg(path).output() {
        Ok(out) => assert!(
            out.status.success(),
            "{} is not valid Lua: {}\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stderr),
            std::fs::read_to_string(path).unwrap_or_default()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "luac is not installed; {} not checked by Lua",
                path.display()
            );
        }
        Err(e) => panic!("cannot run luac: {e}"),
    }
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

// REQ-INSTALL-01: a bindings file that is not UTF-8 is refused before
// anything is installed, and left byte for byte as it was.
#[test]
fn req_install_01_bindings_that_are_not_utf8_are_refused_and_kept() {
    let original = b"-- caf\xe9 in Latin-1\no.bind(\"SUPER + R\", \"Run\", \"run\")\n";
    let (root, bindings) = root_with_bindings("not-utf8", original);
    let out = install_in(&root, &["install"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("ERROR "), "{stderr}");
    assert!(stderr.contains("is not UTF-8"), "{stderr}");
    assert_eq!(std::fs::read(&bindings).expect("read"), original);
    assert!(
        !root
            .home()
            .join(".config/omarchy/plugins/io.github.itsgg.thodu")
            .exists(),
        "a refused install changes nothing"
    );
    let doctor = install_in(&root, &["doctor"]);
    assert_eq!(doctor.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&doctor.stderr).contains("is not UTF-8"),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
}

// REQ-INSTALL-01: a rerun keeps keys written in single quotes or built by
// concatenation, and every other line of the file.
#[test]
fn req_install_01_rerun_keeps_single_quoted_and_concatenated_keys() {
    let original = concat!(
        "local mod = \"SUPER\"\n",
        "o.bind('CTRL + SPACE', 'Thodu hints', 'thodu hints') -- mine\n",
        "o.bind(\"CTRL + \" .. \"SHIFT + SPACE\", \"Thodu grid\", \"thodu grid\")\n",
        "o.bind(mod .. \" + F9\", 'Thodu scroll', 'thodu scroll')\n",
        "o.bind(\"SUPER + R\", \"Run\", \"run\")\n",
    );
    let (root, bindings) = root_with_bindings("quoted-keys", original.as_bytes());
    let out = install_in(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(&bindings).expect("read");
    assert!(
        after.starts_with("local mod = \"SUPER\"\no.bind('CTRL + SPACE', 'Thodu hints', "),
        "{after}"
    );
    assert!(
        after.contains(" hints\") -- mine\n"),
        "a trailing comment stays: {after}"
    );
    assert!(
        after.contains("o.bind(\"CTRL + \" .. \"SHIFT + SPACE\", \"Thodu grid\", "),
        "{after}"
    );
    assert!(
        after.contains("o.bind(mod .. \" + F9\", 'Thodu scroll', "),
        "{after}"
    );
    assert!(
        after.contains("o.bind(\"SUPER + R\", \"Run\", \"run\")\n"),
        "{after}"
    );
    assert!(
        !after.contains("F12"),
        "no default replaces a custom key: {after}"
    );
    assert_eq!(count(&after, "Thodu hints"), 1, "{after}");
    assert_eq!(count(&after, "Thodu grid"), 1, "{after}");
    assert_eq!(count(&after, "Thodu scroll"), 1, "{after}");
    assert!(after.contains(env!("CARGO_BIN_EXE_thodu")), "{after}");
    assert_valid_lua(&bindings);

    let again = install_in(&root, &["install"]);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&bindings).expect("read"),
        after,
        "a rerun is stable"
    );

    let clash = install_in(&root, &["install", "--scroll-key", "CTRL + SPACE"]);
    assert_eq!(
        clash.status.code(),
        Some(2),
        "a key already taken is a bad argument"
    );
    assert_eq!(std::fs::read_to_string(&bindings).expect("read"), after);
}

// REQ-INSTALL-01: a Thodu binding split across lines is replaced whole, and
// a duplicate goes with its line, leaving valid Lua.
#[test]
fn req_install_01_a_binding_split_across_lines_is_replaced_whole() {
    let original = concat!(
        "o.bind(\n",
        "    \"CTRL + SPACE\",\n",
        "    \"Thodu hints\",\n",
        "    \"thodu hints\"\n",
        ")\n",
        "o.bind(\"SUPER + R\", \"Run\", \"run\")\n",
        "o.bind(\"CTRL + H\", \"Thodu hints\",\n  \"old thodu hints\")\n",
    );
    let (root, bindings) = root_with_bindings("split-binding", original.as_bytes());
    let out = install_in(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(&bindings).expect("read");
    assert!(
        after.starts_with("o.bind(\"CTRL + SPACE\", \"Thodu hints\", "),
        "{after}"
    );
    assert_eq!(count(&after, "Thodu hints"), 1, "{after}");
    assert!(
        !after.contains("old thodu hints") && !after.contains("CTRL + H"),
        "{after}"
    );
    assert!(
        after.contains("o.bind(\"SUPER + R\", \"Run\", \"run\")\n"),
        "{after}"
    );
    assert!(
        !after.contains("\n\n"),
        "no blank line is left behind: {after}"
    );
    assert_valid_lua(&bindings);
}

// REQ-INSTALL-01: in a file that ends in `return`, what install adds goes
// before it, where Lua allows statements.
#[test]
fn req_install_01_a_file_ending_in_return_gets_bindings_before_it() {
    let original = concat!(
        "local M = {}\n",
        "function M.setup()\n",
        "  return true\n",
        "end\n",
        "o.bind(\"SUPER + R\", \"Run\", \"run\")\n",
        "return M\n",
    );
    let (root, bindings) = root_with_bindings("ends-in-return", original.as_bytes());
    let out = install_in(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(&bindings).expect("read");
    assert!(after.ends_with("return M\n"), "{after}");
    assert!(
        after.starts_with(&original[..original.len() - "return M\n".len()]),
        "{after}"
    );
    for needle in [
        "Thodu hints",
        "Thodu grid",
        "Thodu scroll",
        "ACCESSIBILITY_ENABLED",
    ] {
        let at = after
            .find(needle)
            .unwrap_or_else(|| panic!("{needle}: {after}"));
        assert!(
            at < after.rfind("return M").expect("return"),
            "{needle}: {after}"
        );
    }
    assert_valid_lua(&bindings);
}

// REQ-INSTALL-01: the bindings are replaced all at once. Under a file size
// limit, as on a full disk, the write fails and the file is left whole; a
// write in place would have truncated it.
#[test]
fn req_install_01_a_failed_bindings_write_leaves_the_file_whole() {
    let mut original = String::from("o.bind(\"SUPER + R\", \"Run\", \"run\")\n");
    while original.len() < 1 << 20 {
        original
            .push_str("-- a long comment line GG keeps in his bindings file, kept byte for byte\n");
    }
    let (root, bindings) = root_with_bindings("full-disk", original.as_bytes());
    let plain = thodu_command(
        &root.home(),
        &root.udev_dir(),
        &["install"],
        &root.0.join("elsewhere"),
    );
    // 200 blocks is 100 or 200 KiB, by the shell's block size: room for
    // every plugin file, none for the 1 MiB bindings file.
    let mut limited = Command::new("sh");
    limited
        .arg("-c")
        .arg("trap '' XFSZ; ulimit -f 200; exec \"$0\" \"$@\"")
        .arg(plain.get_program())
        .args(plain.get_args())
        .current_dir(plain.get_current_dir().expect("working directory"));
    for (key, value) in plain.get_envs() {
        match value {
            Some(value) => limited.env(key, value),
            None => limited.env_remove(key),
        };
    }
    let out = limited.output().expect("run thodu under a file size limit");
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("ERROR "), "{stderr}");
    assert!(stderr.contains("unchanged"), "{stderr}");
    assert!(
        std::fs::read(&bindings).expect("read") == original.as_bytes(),
        "the bindings file is whole"
    );
    let leftovers: Vec<_> = std::fs::read_dir(bindings.parent().expect("parent"))
        .expect("list")
        .map(|e| e.expect("entry").file_name())
        .filter(|name| name != "bindings.lua")
        .collect();
    assert!(
        leftovers.is_empty(),
        "no temporary file is left: {leftovers:?}"
    );
}

// REQ-INSTALL-01: a bindings file kept elsewhere through a symlink is
// updated where it lives, and the link stays.
#[test]
fn req_install_01_a_symlinked_bindings_file_stays_a_link() {
    let (root, bindings) = root_with_bindings("symlinked", b"");
    std::fs::remove_file(&bindings).expect("remove");
    let dotfiles = root.0.join("dotfiles");
    std::fs::create_dir_all(&dotfiles).expect("dotfiles");
    let real = dotfiles.join("bindings.lua");
    std::fs::write(&real, "o.bind(\"SUPER + R\", \"Run\", \"run\")\n").expect("write");
    std::os::unix::fs::symlink(&real, &bindings).expect("link");
    let out = install_in(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(bindings.is_symlink(), "the link is kept");
    let after = std::fs::read_to_string(&real).expect("read");
    assert!(
        after.starts_with("o.bind(\"SUPER + R\", \"Run\", \"run\")\n"),
        "{after}"
    );
    assert!(after.contains("Thodu scroll"), "{after}");
}

// REQ-INSTALL-01: doctor checks the running session's environment, not only
// the file, since apps launched through systemd read the session's copy.
#[test]
fn req_install_01_doctor_checks_the_live_session_environment() {
    let (root, _) = root_with_bindings("live-environment", b"");
    let out = install_in(&root, &["install"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doctor = install_in(&root, &["doctor"]);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stdout)
    );
    // A new login before install, or a session that lost them.
    std::fs::remove_file(
        root.home()
            .join(".local/share/thodu/tests/session-environment"),
    )
    .expect("clear the fake session environment");
    let doctor = install_in(&root, &["doctor"]);
    assert_eq!(doctor.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        stdout.contains("ACCESSIBILITY_ENABLED=1 in Hyprland: missing"),
        "{stdout}"
    );
    assert!(
        stdout.contains("QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 in Hyprland: missing"),
        "{stdout}"
    );
    let stderr = String::from_utf8_lossy(&doctor.stderr);
    assert!(stderr.contains("not in the running session"), "{stderr}");
}

fn copied_thodu(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).expect("create binary directory");
    let copy = dir.join("thodu");
    std::fs::copy(env!("CARGO_BIN_EXE_thodu"), &copy).expect("copy thodu");
    copy
}

// REQ-INSTALL-01: the unit runs thodud by its absolute path, quoted so a
// space, a % or a $ in it reaches systemd as written; with no thodud to
// find, install refuses instead of writing a bare name.
#[test]
fn req_install_01_service_names_thodud_by_a_quoted_absolute_path() {
    let (root, _) = root_with_bindings("odd-path", b"");
    let dir = root.0.join("odd dir 100% $HOME");
    let thodu = copied_thodu(&dir);
    write_executable(&dir.join("thodud"), "#!/bin/sh\nexit 0\n");
    let mut command = thodu_command(
        &root.home(),
        &root.udev_dir(),
        &["install"],
        &root.0.join("elsewhere"),
    );
    let mut quoted = Command::new(&thodu);
    quoted
        .args(command.get_args())
        .current_dir(root.0.join("elsewhere"));
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            quoted.env(key, value);
        }
    }
    let out = quoted.output().expect("run the copied thodu");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let unit = std::fs::read_to_string(root.home().join(".config/systemd/user/thodud.service"))
        .expect("read unit");
    let expected = format!(
        "ExecStart=\"{}/odd dir 100%% $$HOME/thodud\"\n",
        root.0.display()
    );
    assert!(unit.contains(&expected), "{unit}");

    // No thodud beside thodu or on PATH.
    std::fs::remove_file(dir.join("thodud")).expect("remove thodud");
    std::fs::remove_file(root.0.join("bin/thodud")).expect("remove the PATH thodud");
    assert!(!Path::new("/usr/bin/thodud").exists() && !Path::new("/bin/thodud").exists());
    let bin = root.0.join("bin");
    command.env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    let mut bare = Command::new(&thodu);
    bare.args(["install"]).current_dir(root.0.join("elsewhere"));
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            bare.env(key, value);
        }
    }
    let before = std::fs::read_to_string(root.home().join(".config/systemd/user/thodud.service"))
        .expect("read unit");
    let out = bare.output().expect("run the copied thodu without thodud");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("ERROR cannot find thodud"), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(root.home().join(".config/systemd/user/thodud.service"))
            .expect("read unit"),
        before,
        "the unit is not rewritten with a bare name"
    );
}

/// thodu with `args` in a fake home with nothing running, so a command that
/// got past its arguments would fail differently.
fn bad_arguments(args: &[&str]) -> Output {
    let root = TestRoot::new("bad-arguments");
    std::fs::create_dir_all(root.0.join("elsewhere")).expect("working directory");
    install_in(&root, args)
}

// REQ-CLI-01: a bad argument exits 2 with an ERROR line saying what to
// change, before thodu does anything; a value-taking option never takes the
// next flag as its value.
#[test]
fn req_cli_01_bad_arguments_exit_2_with_an_error_line() {
    for (args, says) in [
        (&[][..], "name a command"),
        (&["nonsense"][..], "there is no command"),
        (&["hints", "--window"][..], "--window needs a value"),
        (
            &["hints", "--window", "--test"][..],
            "--window needs a value",
        ),
        (&["targets", "--scope"][..], "--scope needs a value"),
        (
            &["hints", "--scope", "everywhere"][..],
            "--scope must be visible or focused",
        ),
        (
            &["targets", "--scope=all", "--json"][..],
            "--scope must be visible or focused",
        ),
        (&["scroll", "--windwo", "0x1"][..], "has no option --windwo"),
        (&["grid", "--scope", "focused"][..], "has no option --scope"),
        (&["hints", "extra"][..], "takes no argument"),
        (&["hints", "--test", "--test"][..], "--test is given twice"),
        (&["key"][..], "needs SESSION and KEY"),
        (&["key", "s1"][..], "needs KEY"),
        (&["close"][..], "needs SESSION"),
        (
            &["bench", "--sessions", "0"][..],
            "--sessions must be a whole number",
        ),
        (
            &["bench", "--sessions", "many"][..],
            "--sessions must be a whole number",
        ),
        (&["record-tree"][..], "--out FILE is required"),
        (
            &["install", "--hints-key", "SUPER + H"][..],
            "must be supplied together",
        ),
        (
            &[
                "install",
                "--hints-key",
                "SUPER + H",
                "--grid-key",
                "SUPER + G; rm",
            ][..],
            "--grid-key must contain only key names",
        ),
        (
            &[
                "install",
                "--scroll-key",
                "SUPER + ALT + F12",
                "--hints-key",
                "A",
                "--grid-key",
                "A",
            ][..],
            "must be different",
        ),
        (&["doctor", "--fix"][..], "has no option --fix"),
    ] {
        let out = bad_arguments(args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {stderr}");
        let first = stderr.lines().next().unwrap_or_default();
        assert!(first.starts_with("ERROR "), "{args:?}: {stderr}");
        assert!(!first.starts_with("ERROR ERROR"), "{args:?}: {stderr}");
        assert!(first.contains(says), "{args:?}: {stderr}");
        assert!(stderr.contains("usage:"), "{args:?}: {stderr}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
}

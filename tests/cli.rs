use std::{fs, os::unix::fs::PermissionsExt, process::Command};

fn command(temp: &tempfile::TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_arch-cast"));
    command
        .env("HOME", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join("config"))
        .env("XDG_STATE_HOME", temp.path().join("state"))
        .env("XDG_DATA_HOME", temp.path().join("data"))
        .env("XDG_DATA_DIRS", temp.path().join("system"))
        .env("XDG_RUNTIME_DIR", temp.path().join("runtime"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY");
    command
}
#[test]
fn list_works_without_a_display_and_does_not_start_the_resident() {
    let temp = tempfile::tempdir().unwrap();
    let apps = temp.path().join("data/applications");
    fs::create_dir_all(&apps).unwrap();
    fs::write(
        apps.join("browser.desktop"),
        "[Desktop Entry]\nType=Application\nName=Test Browser\nExec=true\nKeywords=web;\n",
    )
    .unwrap();
    let output = command(&temp).args(["list", "web"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows[0]["id"], "app:browser.desktop");
    assert!(!temp.path().join("runtime/arch-cast.sock").exists());
    assert!(!temp.path().join("state").exists());
}
#[test]
fn hide_and_quit_do_not_start_a_missing_resident() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = temp.path().join("runtime");
    fs::create_dir(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    for action in ["hide", "quit"] {
        assert!(command(&temp).arg(action).status().unwrap().success());
    }
    assert!(!runtime.join("arch-cast.sock").exists());
    assert!(!temp.path().join("state").exists());
}
#[test]
fn run_requires_an_id_and_doctor_reports_a_missing_display() {
    let temp = tempfile::tempdir().unwrap();
    let output = command(&temp).arg("run").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires an entry ID"));
    let output = command(&temp).arg("doctor").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Display: MISSING"));
    assert!(!temp.path().join("runtime/arch-cast.sock").exists());
}

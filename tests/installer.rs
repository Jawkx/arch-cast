use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::{Command, Output},
};

fn script(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for dir in ["home/.config/hypr", "home/.local/bin", "tools", "runtime"] {
        fs::create_dir_all(temp.path().join(dir)).unwrap();
    }
    fs::set_permissions(
        temp.path().join("runtime"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    fs::write(
        temp.path().join("home/.config/hypr/hyprland.lua"),
        "-- My desktop\nlocal menu = 'rofi -show drun'\n",
    )
    .unwrap();
    script(&temp.path().join("tools/hyprctl"), "#!/bin/sh\nexit 0\n");
    script(
        &temp.path().join("tools/update-desktop-database"),
        "#!/bin/sh\nexit 0\n",
    );
    temp
}
fn run(temp: &tempfile::TempDir, command: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arch-cast"))
        .arg(command)
        .env("HOME", temp.path().join("home"))
        .env("XDG_CONFIG_HOME", temp.path().join("home/.config"))
        .env("XDG_STATE_HOME", temp.path().join("state"))
        .env("XDG_DATA_HOME", temp.path().join("data"))
        .env("XDG_RUNTIME_DIR", temp.path().join("runtime"))
        .env(
            "PATH",
            format!("{}:/usr/bin", temp.path().join("tools").display()),
        )
        .output()
        .unwrap()
}
#[test]
fn install_reinstall_and_uninstall_preserve_original_menu_and_user_edits() {
    let temp = fixture();
    let root = temp.path();
    let output = run(&temp, "install");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bin = root.join("home/.local/bin/arch-cast");
    assert!(!fs::symlink_metadata(&bin).unwrap().is_symlink());
    assert_eq!(
        fs::read(&bin).unwrap(),
        fs::read(env!("CARGO_BIN_EXE_arch-cast")).unwrap()
    );
    let manifest = root.join("home/.config/arch-cast/install.json");
    let first = fs::read(&manifest).unwrap();
    let output = run(&temp, "install");
    assert!(output.status.success());
    assert_eq!(fs::read(&manifest).unwrap(), first);
    let hypr = root.join("home/.config/hypr/hyprland.lua");
    let source = fs::read_to_string(&hypr).unwrap();
    fs::write(&hypr, format!("{source}\n-- Later user edit\n")).unwrap();
    let output = run(&temp, "uninstall");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = fs::read_to_string(&hypr).unwrap();
    assert!(source.contains("local menu = 'rofi -show drun'"));
    assert!(source.contains("-- Later user edit"));
    assert!(!source.contains("BEGIN ARCH CAST"));
    assert!(!bin.exists());
    assert!(!manifest.exists());
    assert!(root.join("home/.config/arch-cast/config.json").exists());
}
#[test]
fn compositor_rejection_rolls_back_binary_link_config_and_desktop_files() {
    let temp = fixture();
    let root = temp.path();
    let previous = root.join("previous-launcher");
    fs::write(&previous, "previous executable").unwrap();
    let bin = root.join("home/.local/bin/arch-cast");
    symlink(&previous, &bin).unwrap();
    let hypr = root.join("home/.config/hypr/hyprland.lua");
    let before = fs::read(&hypr).unwrap();
    script(
        &root.join("tools/hyprctl"),
        "#!/bin/sh\nif [ \"$1\" = configerrors ]; then printf 'bad rule\\n'; fi\nexit 0\n",
    );
    let output = run(&temp, "install");
    assert!(!output.status.success());
    assert_eq!(fs::read(&hypr).unwrap(), before);
    assert_eq!(fs::read_link(&bin).unwrap(), previous);
    assert!(!root.join("home/.config/arch-cast/config.json").exists());
    assert!(!root.join("home/.config/arch-cast/install.json").exists());
    assert!(!root.join("data/applications/arch-cast.desktop").exists());
    assert!(root.join("home/.config/arch-cast/backups").is_dir());
}

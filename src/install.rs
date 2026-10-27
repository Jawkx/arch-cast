use crate::{
    catalog::Config,
    ipc::{self, Command as IpcCommand, Request},
    paths::Paths,
    storage::write_json,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

const START: &str = "-- BEGIN ARCH CAST";
const END: &str = "-- END ARCH CAST";
const BLOCK: &str = r#"-- BEGIN ARCH CAST
-- Standalone GPUI launcher. The shortcut remains owned by Hyprland.
hl.window_rule({
    name = "arch-cast-launcher",
    match = { class = "^(arch-cast)$" },
    border_size = 0,
    float = true,
    center = true,
    dim_around = true,
    animation = "popin 95%",
})
hl.on("hyprland.start", function()
    hl.exec_cmd("$HOME/.local/bin/arch-cast daemon")
end)
-- END ARCH CAST
"#;
const MENU: &str = "local menu = '$HOME/.local/bin/arch-cast toggle'";
#[derive(Serialize, Deserialize)]
struct Manifest {
    hypr_backup: PathBuf,
    previous_menu: String,
    #[serde(default)]
    root: Option<PathBuf>,
}

fn hypr_path(paths: &Paths) -> PathBuf {
    paths.config.parent().unwrap().join("hypr/hyprland.lua")
}
fn menu_line(source: &str) -> Result<&str> {
    let lines: Vec<_> = source
        .lines()
        .filter(|line| line.trim_start().starts_with("local menu = "))
        .collect();
    ensure!(
        lines.len() == 1,
        "expected exactly one 'local menu = ' line in hyprland.lua"
    );
    Ok(lines[0])
}
fn remove_block(source: &str) -> Result<String> {
    ensure!(
        source.matches(START).count() <= 1 && source.matches(END).count() <= 1,
        "multiple Arch Cast blocks, refusing to edit"
    );
    match (source.find(START), source.find(END)) {
        (None, None) => Ok(source.into()),
        (Some(start), Some(end)) if end >= start => {
            let end = end + END.len();
            let end = if source[end..].starts_with('\n') {
                end + 1
            } else {
                end
            };
            Ok(format!("{}{}", &source[..start], &source[end..]))
        }
        _ => bail!("incomplete Arch Cast block, refusing to edit"),
    }
}
fn installed_config(source: &str) -> Result<(String, String)> {
    let old = menu_line(source)?;
    ensure!(
        old.contains("rofi") || old.contains("arch-cast"),
        "menu does not refer to Rofi or Arch Cast; no changes made"
    );
    let text = source.replacen(old, MENU, 1);
    let text = remove_block(&text)?;
    Ok((format!("{}\n\n{BLOCK}", text.trim_end()), old.into()))
}
fn restored_config(source: &str, previous: &str) -> Result<String> {
    let text = remove_block(source)?;
    // Preserve a launcher binding the user changed after installing.
    let text = if text.lines().any(|l| l == MENU) {
        text.replacen(MENU, previous, 1)
    } else {
        text
    };
    Ok(format!("{}\n", text.trim_end()))
}

fn reload() -> Result<()> {
    let output = Command::new("hyprctl")
        .arg("reload")
        .output()
        .context("run hyprctl reload")?;
    ensure!(
        output.status.success(),
        "Hyprland reload failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let errors = Command::new("hyprctl").arg("configerrors").output()?;
    ensure!(
        errors.status.success() && errors.stdout.trim_ascii().is_empty(),
        "Hyprland rejected config: {}",
        String::from_utf8_lossy(&errors.stdout)
    );
    Ok(())
}

enum Snapshot {
    Missing,
    File(Vec<u8>, u32),
    Link(PathBuf),
}
impl Snapshot {
    fn read(path: &Path) -> Result<Self> {
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::Missing),
            Err(e) => Err(e.into()),
            Ok(m) if m.is_symlink() => Ok(Self::Link(fs::read_link(path)?)),
            Ok(m) => Ok(Self::File(fs::read(path)?, m.permissions().mode())),
        }
    }
    fn restore(&self, path: &Path) -> Result<()> {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        match self {
            Self::Missing => Ok(()),
            Self::Link(target) => Ok(symlink(target, path)?),
            Self::File(bytes, mode) => {
                fs::write(path, bytes)?;
                fs::set_permissions(path, fs::Permissions::from_mode(*mode))?;
                Ok(())
            }
        }
    }
}
fn write_executable(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination.parent().unwrap())?;
    let mut file = NamedTempFile::new_in(destination.parent().unwrap())?;
    std::io::copy(&mut fs::File::open(source)?, &mut file)?;
    file.flush()?;
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o755))?;
    file.as_file().sync_all()?;
    file.persist(destination).map_err(|e| e.error)?;
    Ok(())
}
fn update_desktop_database(paths: &Paths) {
    let _ = Command::new("update-desktop-database")
        .arg(paths.data.join("applications"))
        .output();
}

pub fn install(paths: &Paths) -> Result<()> {
    let hypr = hypr_path(paths);
    let source = fs::read_to_string(&hypr).with_context(|| format!("read {}", hypr.display()))?;
    let (text, previous_menu) = installed_config(&source)?;
    let backup = paths.config.join("backups").join(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
            .to_string(),
    );
    fs::create_dir_all(&backup)?;
    fs::copy(&hypr, backup.join("hyprland.lua"))?;
    let bin = paths.home.join(".local/bin/arch-cast");
    let desktop = paths.data.join("applications/arch-cast.desktop");
    let manifest = paths.config.join("install.json");
    let config = paths.config.join("config.json");
    if !manifest.exists() && bin.exists() {
        fs::copy(&bin, backup.join("arch-cast.previous"))?;
    }
    let targets = [
        hypr.clone(),
        bin.clone(),
        desktop.clone(),
        manifest.clone(),
        config.clone(),
    ];
    let snapshots: Vec<_> = targets
        .iter()
        .map(|p| Snapshot::read(p))
        .collect::<Result<_>>()?;
    let operation = (|| -> Result<()> {
        write_executable(&std::env::current_exe()?, &bin)?;
        fs::create_dir_all(desktop.parent().unwrap())?;
        let exec = bin
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%");
        fs::write(
            &desktop,
            format!(
                "[Desktop Entry]\nType=Application\nName=Arch Cast\nComment=Search apps and commands\nExec=\"{exec}\" show\nIcon=system-search\nTerminal=false\nCategories=Utility;\nStartupWMClass=arch-cast\n"
            ),
        )?;
        if !config.exists() {
            write_json(&config, &Config::default())?;
        }
        if !manifest.exists() {
            write_json(
                &manifest,
                &Manifest {
                    hypr_backup: backup.join("hyprland.lua"),
                    previous_menu,
                    root: None,
                },
            )?;
        }
        fs::write(&hypr, text)?;
        reload()?;
        Ok(())
    })();
    if let Err(error) = operation {
        let mut rollback_errors = vec![];
        for (path, snapshot) in targets.iter().zip(&snapshots) {
            if let Err(e) = snapshot.restore(path) {
                rollback_errors.push(format!("{}: {e}", path.display()));
            }
        }
        if let Err(e) = reload() {
            rollback_errors.push(e.to_string());
        }
        if !rollback_errors.is_empty() {
            bail!(
                "{error:#}; rollback also failed: {}. Backup: {}",
                rollback_errors.join("; "),
                backup.display()
            );
        }
        return Err(error.context(format!(
            "installation rolled back; backup: {}",
            backup.display()
        )));
    }
    let _ = ipc::send(&paths.runtime, &Request::new(IpcCommand::Quit));
    update_desktop_database(paths);
    println!(
        "Installed {}\nSuper+Space opens Arch Cast.\nBackup: {}",
        bin.display(),
        backup.display()
    );
    Ok(())
}

pub fn uninstall(paths: &Paths) -> Result<()> {
    let manifest_path = paths.config.join("install.json");
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(&manifest_path).context("no install manifest; no changes made")?,
    )?;
    let hypr = hypr_path(paths);
    let source = fs::read_to_string(&hypr)?;
    let text = restored_config(&source, &manifest.previous_menu)?;
    fs::write(&hypr, text)?;
    if let Err(error) = reload() {
        fs::write(&hypr, source)?;
        let _ = reload();
        return Err(error.context("uninstall rolled back"));
    }
    let _ = ipc::send(&paths.runtime, &Request::new(IpcCommand::Quit));
    for path in [
        paths.home.join(".local/bin/arch-cast"),
        paths.data.join("applications/arch-cast.desktop"),
    ] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    if let Some(backup_dir) = manifest.hypr_backup.parent() {
        let previous = backup_dir.join("arch-cast.previous");
        if previous.is_file() {
            write_executable(&previous, &paths.home.join(".local/bin/arch-cast"))?;
        }
    }
    fs::remove_file(manifest_path)?;
    update_desktop_database(paths);
    println!(
        "Restored the previous launcher. Kept settings and backups in {}",
        paths.config.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn install_is_idempotent_and_uninstall_preserves_unrelated_edits() {
        let original = "-- My config\nlocal menu = 'rofi -show drun'\n-- binding remains here\n";
        let (installed, old) = installed_config(original).unwrap();
        assert_eq!(old, "local menu = 'rofi -show drun'");
        assert_eq!(installed.matches(START).count(), 1);
        assert!(
            installed
                .contains("    match = { class = \"^(arch-cast)$\" },\n    border_size = 0,\n")
        );
        assert_eq!(installed_config(&installed).unwrap().0, installed);
        let changed = format!("{installed}\n-- Added later\n");
        let restored = restored_config(&changed, &old).unwrap();
        assert!(restored.contains(&old));
        assert!(restored.contains("-- Added later"));
        assert!(!restored.contains(START));
        let changed = changed.replace(MENU, "local menu = 'another-launcher'");
        assert!(
            restored_config(&changed, &old)
                .unwrap()
                .contains("local menu = 'another-launcher'")
        );
    }
    #[test]
    fn rejects_ambiguous_or_incomplete_config() {
        assert!(installed_config("local menu = 'custom'\n").is_err());
        assert!(installed_config("local menu = 'rofi'\nlocal menu = 'rofi'\n").is_err());
        assert!(installed_config("local menu = 'rofi'\n-- BEGIN ARCH CAST\n").is_err());
    }
    #[test]
    fn snapshot_restores_links_and_atomic_install_replaces_a_link() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::write(&source, "binary").unwrap();
        let dest = temp.path().join("dest");
        symlink(&source, &dest).unwrap();
        let snapshot = Snapshot::read(&dest).unwrap();
        write_executable(&source, &dest).unwrap();
        assert!(!fs::symlink_metadata(&dest).unwrap().is_symlink());
        assert_eq!(
            fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
            0o755
        );
        snapshot.restore(&dest).unwrap();
        assert_eq!(fs::read_link(&dest).unwrap(), source);
    }
}

use crate::{
    catalog::{Catalog, Entry, EntryKind},
    desktop,
    paths::Paths,
};
use anyhow::{Context, Result, bail};
use std::{
    fs::OpenOptions,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// No inherited terminal, process group, or child zombies after a launch.
pub fn detached(mut command: Command) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and touches no Rust state after fork.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().context("start detached application")?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn launch(entry: &Entry, catalog: &Catalog) -> Result<()> {
    let mut command;
    if let Some(id) = entry.id.strip_prefix("app:") {
        command = Command::new("gtk-launch");
        command.arg(id);
    } else if entry.id.starts_with("custom:") {
        // Only explicit configuration is shell code. Search text never reaches this branch.
        command = Command::new("/bin/sh");
        command.args(["-c", &entry.target]);
    } else if entry.kind == EntryKind::WebSearch {
        command = Command::new("xdg-open");
        command.arg(&entry.target);
    } else if entry.id == "config" {
        if !catalog.paths.config.join("config.json").exists() {
            catalog.save_config()?;
        }
        command = Command::new("xdg-open");
        command.arg(catalog.paths.config.join("config.json"));
    } else {
        bail!("entry needs a launcher window: {}", entry.id);
    }
    detached(command)
}

pub fn icon_path(name: &str) -> Option<PathBuf> {
    if Path::new(name).is_absolute() {
        return Path::new(name).is_file().then(|| name.into());
    }
    for theme in ["Papirus-Dark", "breeze-dark", "Adwaita", "hicolor"] {
        if let Some(icon) = freedesktop_icons::lookup(name)
            .with_size(48)
            .with_theme(theme)
            .find()
        {
            return Some(icon);
        }
    }
    None
}

pub fn focus_launcher() {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        let mut cmd = Command::new("hyprctl");
        cmd.args([
            "dispatch",
            "hl.dsp.focus({ window = \"class:^(arch-cast)$\" })",
        ]);
        if let Err(error) = detached(cmd) {
            eprintln!("arch-cast: focus: {error:#}");
        }
    }
}

pub fn start_resident(paths: &Paths) -> Result<std::process::Child> {
    std::fs::create_dir_all(&paths.state)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.state.join("launcher.log"))?;
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("daemon")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(cmd.spawn()?)
}

pub fn doctor(paths: &Paths) -> Result<()> {
    let mut missing = vec![];
    println!("UI: GPUI Kit 0.8 / GPUI 0.3.9");
    println!(
        "Desktop: {}",
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into())
    );
    println!(
        "Display: {}",
        std::env::var("WAYLAND_DISPLAY")
            .or_else(|_| std::env::var("DISPLAY"))
            .unwrap_or_else(|_| {
                missing.push("display");
                "MISSING".into()
            })
    );
    for tool in ["gtk-launch", "xdg-open"] {
        let found = desktop::executable(tool);
        println!(
            "{tool}: {}",
            if found {
                "available"
            } else {
                missing.push(tool);
                "MISSING"
            }
        );
    }
    println!(
        "Resident: {}",
        if crate::ipc::send(
            &paths.runtime,
            &crate::ipc::Request::new(crate::ipc::Command::Ping)
        )
        .is_ok()
        {
            "running"
        } else {
            "not running, starts on first use"
        }
    );
    println!(
        "Applications: {}",
        Catalog::new(paths.clone())
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Application)
            .count()
    );
    println!("Config: {}", paths.config.join("config.json").display());
    if !missing.is_empty() {
        bail!("missing requirements: {}", missing.join(", "));
    }
    Ok(())
}

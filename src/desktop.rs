use crate::catalog::{Entry, EntryKind};
use std::{
    collections::{HashMap, HashSet},
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub fn executable(name: &str) -> bool {
    let is_exec = |p: &Path| {
        p.is_file()
            && p.metadata()
                .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    };
    if name.contains('/') {
        return is_exec(Path::new(name));
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|p| is_exec(&p.join(name))))
}

/// The first desktop ID wins, even when that file hides a system entry.
pub fn discover(directories: &[PathBuf], desktop: &str, locale: &str) -> Vec<Entry> {
    let mut seen = HashSet::new();
    let desktops: HashSet<_> = desktop.split(':').filter(|s| !s.is_empty()).collect();
    let locale = locale.split('.').next().unwrap_or(locale);
    let language = locale.split('_').next().unwrap_or(locale);
    let mut entries = Vec::new();
    for dir in directories {
        let mut files: Vec<_> = WalkDir::new(dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "desktop")
            })
            .map(|e| e.into_path())
            .collect();
        files.sort();
        for path in files {
            let id = path
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .replace('/', "-");
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            let item = parse_entry(&source);
            let get = |k: &str| item.get(k).map(String::as_str).unwrap_or("");
            let flag = |k: &str| get(k).eq_ignore_ascii_case("true");
            if !matches!(get("Type"), "" | "Application") || flag("Hidden") || flag("NoDisplay") {
                continue;
            }
            if !get("OnlyShowIn").is_empty()
                && !get("OnlyShowIn").split(';').any(|d| desktops.contains(d))
            {
                continue;
            }
            if get("NotShowIn")
                .split(';')
                .any(|d| !d.is_empty() && desktops.contains(d))
            {
                continue;
            }
            if !get("TryExec").is_empty() && !executable(get("TryExec")) {
                continue;
            }
            if get("Exec").is_empty() && !flag("DBusActivatable") {
                continue;
            }
            let localized = |key: &str| {
                [
                    format!("{key}[{locale}]"),
                    format!("{key}[{language}]"),
                    key.into(),
                ]
                .iter()
                .find_map(|k| item.get(k).filter(|v| !v.is_empty()).cloned())
                .unwrap_or_default()
            };
            let name = localized("Name");
            if name.is_empty() {
                continue;
            }
            entries.push(Entry {
                id: format!("app:{id}"),
                name,
                kind: EntryKind::Application,
                detail: localized("GenericName"),
                icon: if get("Icon").is_empty() {
                    "application-x-executable".into()
                } else {
                    get("Icon").into()
                },
                target: path.to_string_lossy().into_owned(),
                keywords: format!(
                    "{} {} {} {}",
                    localized("Keywords"),
                    localized("Comment"),
                    get("Exec"),
                    id
                ),
            });
        }
    }
    entries.sort_by_key(|e| e.name.to_lowercase());
    entries
}

fn parse_entry(source: &str) -> HashMap<String, String> {
    let mut active = false;
    let mut values = HashMap::new();
    for line in source.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            active = line == "[Desktop Entry]";
            continue;
        }
        if active && let Some((key, value)) = line.split_once('=') {
            let mut decoded = String::new();
            let mut chars = value.trim().chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    match chars.next() {
                        Some('s') => decoded.push(' '),
                        Some('n') => decoded.push('\n'),
                        Some('t') => decoded.push('\t'),
                        Some('r') => decoded.push('\r'),
                        Some(c) => decoded.push(c),
                        None => decoded.push('\\'),
                    }
                } else {
                    decoded.push(c);
                }
            }
            values.insert(key.trim().into(), decoded);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xdg_precedence_filters_and_localization() {
        let tmp = tempfile::tempdir().unwrap();
        let user = tmp.path().join("user");
        let system = tmp.path().join("system");
        let write = |dir: &Path, file: &str, name: &str, extra: &str| {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                format!("[Desktop Entry]\nType=Application\nName={name}\nExec=true\n{extra}"),
            )
            .unwrap();
        };
        write(&system, "browser.desktop", "System browser", "");
        write(&user, "browser.desktop", "User browser", "");
        write(&system, "hidden.desktop", "Should not appear", "");
        write(&user, "hidden.desktop", "Hidden override", "Hidden=true\n");
        write(&system, "private.desktop", "Private", "NoDisplay=true\n");
        write(
            &system,
            "tools/editor.desktop",
            "Nested editor",
            "Name[fr]=Éditeur\n",
        );
        write(
            &system,
            "missing.desktop",
            "Missing",
            "TryExec=arch-cast-impossible-command-xyz\n",
        );
        write(&system, "kde.desktop", "KDE only", "OnlyShowIn=KDE;\n");
        write(&system, "not.desktop", "Not here", "NotShowIn=Hyprland;\n");
        write(
            &system,
            "actions.desktop",
            "Real app",
            "[Desktop Action New]\nName=Wrong name\n",
        );
        let apps = discover(&[user, system], "Hyprland", "fr_FR.UTF-8");
        assert_eq!(
            apps.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["Real app", "User browser", "Éditeur"]
        );
        assert!(apps.iter().any(|e| e.id == "app:tools-editor.desktop"));
    }
}

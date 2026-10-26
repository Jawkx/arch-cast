use std::{env, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    pub runtime: PathBuf,
    pub data: PathBuf,
    pub home: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be set");
        let xdg = |key, fallback: &str| {
            env::var_os(key)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(fallback))
        };
        Self {
            config: xdg("XDG_CONFIG_HOME", ".config").join("arch-cast"),
            state: xdg("XDG_STATE_HOME", ".local/state").join("arch-cast"),
            data: xdg("XDG_DATA_HOME", ".local/share"),
            runtime: env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    env::temp_dir().join(format!("arch-cast-{}", unsafe { libc::getuid() }))
                }),
            home,
        }
    }

    pub fn applications(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.data.join("applications")];
        let system =
            env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        dirs.extend(
            env::split_paths(&system)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| p.join("applications")),
        );
        dirs
    }
}

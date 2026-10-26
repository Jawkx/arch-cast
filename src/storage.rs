use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{fs, io::Write, path::Path};
use tempfile::NamedTempFile;

/// Write alongside the destination, then rename. A crash never leaves half a JSON file.
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("JSON path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temp = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temp, value)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn read_object(path: &Path) -> Map<String, Value> {
    fs::read(path)
        .ok()
        .and_then(|s| serde_json::from_slice::<Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

pub fn take<T: serde::de::DeserializeOwned>(
    map: &mut Map<String, Value>,
    key: &str,
    fallback: T,
) -> T {
    map.remove(key)
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or(fallback)
}

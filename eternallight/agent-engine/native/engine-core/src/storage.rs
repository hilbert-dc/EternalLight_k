use crate::Result;
use fs2::FileExt;
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub(crate) const MAX_DOCUMENT_BYTES: u64 = 32 * 1024 * 1024;

pub(crate) fn lock_file(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|e| e.to_string())
}

pub(crate) fn try_own(path: &Path) -> Result<Option<File>> {
    let file = lock_file(path)?;
    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn read_json(path: &Path) -> Result<Option<Value>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err("engine document exceeds storage limit".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "invalid engine storage document".into())
}

pub fn durable_replace_json(path: &Path, document: &Value) -> Result<()> {
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(document).map_err(|_| "invalid engine document")?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err("engine document exceeds storage limit".into());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.write_all(b"\n"))
        .map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.error.to_string())?;
    // Windows does not support opening directories using File::open for fsync.
    #[cfg(unix)]
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

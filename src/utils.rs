use std::path::{Path, PathBuf};

use bytes::Bytes;

pub fn wine_path(path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    format!("Z:{}", abs.to_string_lossy().replace('/', "\\"))
}

/// Convert a line of output to UTF-8 like the PHP implementation does: valid UTF-8 is kept
/// as-is, anything else is treated as ISO-8859-1 (`utf8_encode` in PHP).
pub fn to_utf8(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(value) => value.to_string(),
        Err(_) => bytes.iter().map(|&byte| byte as char).collect(),
    }
}

pub fn md5_hex(data: Bytes) -> String {
    md5::compute(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn find_module_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    let lower = name.to_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().to_lowercase() == lower)
        .map(|e| e.path())
}

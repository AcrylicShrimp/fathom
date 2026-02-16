use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;

use super::super::error::FsError;

pub(crate) fn map_io_error(error: io::Error) -> FsError {
    match error.kind() {
        io::ErrorKind::NotFound => FsError::not_found(error.to_string()),
        io::ErrorKind::PermissionDenied => FsError::permission_denied(error.to_string()),
        io::ErrorKind::AlreadyExists => FsError::already_exists(error.to_string()),
        io::ErrorKind::IsADirectory => FsError::not_file(error.to_string()),
        io::ErrorKind::NotADirectory => FsError::not_directory(error.to_string()),
        _ => FsError::io_error(error.to_string()),
    }
}

pub(crate) fn path_for_output(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.is_empty() {
        ".".to_string()
    } else {
        value
    }
}

pub(crate) fn is_hidden_name(name: &OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

pub(crate) fn read_utf8_file(path: &Path, normalized_path: &str) -> Result<String, FsError> {
    let bytes = fs::read(path).map_err(map_io_error)?;
    String::from_utf8(bytes).map_err(|error| {
        FsError::invalid_encoding(format!(
            "`{normalized_path}` is not a valid UTF-8 text file: {error}"
        ))
    })
}

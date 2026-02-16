mod common;
mod glob;
mod list;
mod read;
mod replace;
mod search;
mod write;

use serde_json::Value;

use super::ReplaceMode;
use super::error::FsError;
use super::path::ParsedPath;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ListOptions {
    pub(crate) recursive: bool,
    pub(crate) max_entries: usize,
    pub(crate) include_hidden: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReadOptions {
    pub(crate) offset_line: usize,
    pub(crate) limit_lines: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GlobOptions {
    pub(crate) max_results: usize,
    pub(crate) include_hidden: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchOptions {
    pub(crate) include: Vec<String>,
    pub(crate) max_results: usize,
    pub(crate) case_sensitive: bool,
}

pub(crate) fn list(
    path: &ParsedPath,
    options: ListOptions,
    environment_state: &Value,
) -> Result<Value, FsError> {
    list::list(path, options, environment_state)
}

pub(crate) fn read(
    path: &ParsedPath,
    options: ReadOptions,
    environment_state: &Value,
) -> Result<Value, FsError> {
    read::read(path, options, environment_state)
}

pub(crate) fn write(
    path: &ParsedPath,
    content: &str,
    allow_override: bool,
    create_parents: bool,
    environment_state: &Value,
) -> Result<Value, FsError> {
    write::write(
        path,
        content,
        allow_override,
        create_parents,
        environment_state,
    )
}

pub(crate) fn replace(
    path: &ParsedPath,
    old: &str,
    new: &str,
    mode: ReplaceMode,
    expected_replacements: Option<usize>,
    environment_state: &Value,
) -> Result<Value, FsError> {
    replace::replace(
        path,
        old,
        new,
        mode,
        expected_replacements,
        environment_state,
    )
}

pub(crate) fn glob(
    path: &ParsedPath,
    pattern: &str,
    options: GlobOptions,
    environment_state: &Value,
) -> Result<Value, FsError> {
    glob::glob(path, pattern, options, environment_state)
}

pub(crate) fn search(
    path: &ParsedPath,
    pattern: &str,
    options: SearchOptions,
    environment_state: &Value,
) -> Result<Value, FsError> {
    search::search(path, pattern, options, environment_state)
}

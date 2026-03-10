mod error;
mod path;
mod real;
mod result;

use fathom_capability_domain::ActionOutcome;
use serde::Deserialize;
use serde_json::{Value, json};

use self::error::FsError;
use self::path::{ParsedPath, parse_path, resolve_base_path};
use self::real::{GlobOptions, ListOptions, ReadOptions, SearchOptions};

const LIST_DEFAULT_MAX_ENTRIES: usize = 200;
const LIST_MAX_ENTRIES_CAP: usize = 5_000;
const READ_DEFAULT_OFFSET_LINE: usize = 1;
const READ_DEFAULT_LIMIT_LINES: usize = 200;
const READ_MAX_LIMIT_LINES: usize = 2_000;
const GLOB_DEFAULT_MAX_RESULTS: usize = 500;
const GLOB_MAX_RESULTS_CAP: usize = 5_000;
const SEARCH_DEFAULT_MAX_RESULTS: usize = 200;
const SEARCH_MAX_RESULTS_CAP: usize = 10_000;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReplaceMode {
    First,
    All,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    path: String,
    recursive: Option<bool>,
    max_entries: Option<u64>,
    include_hidden: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    path: String,
    offset_line: Option<u64>,
    limit_lines: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArgs {
    path: String,
    content: String,
    allow_override: bool,
    create_parents: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceArgs {
    path: String,
    old: String,
    new: String,
    mode: ReplaceMode,
    expected_replacements: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobArgs {
    pattern: String,
    path: Option<String>,
    max_results: Option<u64>,
    include_hidden: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    pattern: String,
    path: Option<String>,
    include: Option<Vec<String>>,
    max_results: Option<u64>,
    case_sensitive: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetBasePathArgs {}

pub fn execute_action(
    action_name: &str,
    args_json: &str,
    capability_domain_state: &Value,
) -> Option<ActionOutcome> {
    match action_name {
        "get_base_path" => Some(execute_get_base_path(args_json, capability_domain_state)),
        "list" => Some(execute_list(args_json, capability_domain_state)),
        "read" => Some(execute_read(args_json, capability_domain_state)),
        "write" => Some(execute_write(args_json, capability_domain_state)),
        "replace" => Some(execute_replace(args_json, capability_domain_state)),
        "glob" => Some(execute_glob(args_json, capability_domain_state)),
        "search" => Some(execute_search(args_json, capability_domain_state)),
        _ => None,
    }
}

fn execute_get_base_path(args_json: &str, capability_domain_state: &Value) -> ActionOutcome {
    if let Err(error) = parse_args::<GetBasePathArgs>(args_json, "filesystem__get_base_path") {
        return result::failure("get_base_path", Some("."), &error, Some("filesystem"));
    }

    match resolve_base_path(capability_domain_state) {
        Ok(base_path) => result::success(
            "get_base_path",
            ".",
            "filesystem",
            json!({
                "base_path": base_path.display().to_string(),
                "source": "filesystem_env_state"
            }),
        ),
        Err(error) => result::failure("get_base_path", Some("."), &error, Some("filesystem")),
    }
}

fn execute_list(args_json: &str, capability_domain_state: &Value) -> ActionOutcome {
    let args = match parse_args::<ListArgs>(args_json, "filesystem__list") {
        Ok(args) => args,
        Err(error) => return result::failure("list", None, &error, None),
    };
    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("list", Some(&args.path), &error, None),
    };
    let options = match parse_list_options(args) {
        Ok(options) => options,
        Err(error) => {
            return result::failure(
                "list",
                Some(parsed.normalized_path()),
                &error,
                Some("filesystem"),
            );
        }
    };

    execute_list_on_path(parsed, options, capability_domain_state)
}

fn execute_read(args_json: &str, capability_domain_state: &Value) -> ActionOutcome {
    let args = match parse_args::<ReadArgs>(args_json, "filesystem__read") {
        Ok(args) => args,
        Err(error) => return result::failure("read", None, &error, None),
    };
    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("read", Some(&args.path), &error, None),
    };
    let options = match parse_read_options(args) {
        Ok(options) => options,
        Err(error) => {
            return result::failure(
                "read",
                Some(parsed.normalized_path()),
                &error,
                Some("filesystem"),
            );
        }
    };

    execute_read_on_path(parsed, options, capability_domain_state)
}

fn execute_write(args_json: &str, capability_domain_state: &Value) -> ActionOutcome {
    let args = match parse_args::<WriteArgs>(args_json, "filesystem__write") {
        Ok(args) => args,
        Err(error) => return result::failure("write", None, &error, None),
    };
    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("write", Some(&args.path), &error, None),
    };

    execute_write_on_path(
        parsed,
        &args.content,
        args.allow_override,
        args.create_parents.unwrap_or(true),
        capability_domain_state,
    )
}

fn execute_replace(args_json: &str, capability_domain_state: &Value) -> ActionOutcome {
    let args = match parse_args::<ReplaceArgs>(args_json, "filesystem__replace") {
        Ok(args) => args,
        Err(error) => return result::failure("replace", None, &error, None),
    };
    let parsed = match parse_path(&args.path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("replace", Some(&args.path), &error, None),
    };
    let expected_replacements = match parse_optional_usize(
        args.expected_replacements,
        "filesystem__replace",
        "expected_replacements",
        0,
        usize::MAX,
    ) {
        Ok(value) => value,
        Err(error) => {
            return result::failure(
                "replace",
                Some(parsed.normalized_path()),
                &error,
                Some("filesystem"),
            );
        }
    };

    execute_replace_on_path(
        parsed,
        &args.old,
        &args.new,
        args.mode,
        expected_replacements,
        capability_domain_state,
    )
}

fn execute_glob(args_json: &str, capability_domain_state: &Value) -> ActionOutcome {
    let args = match parse_args::<GlobArgs>(args_json, "filesystem__glob") {
        Ok(args) => args,
        Err(error) => return result::failure("glob", None, &error, None),
    };
    let path = args.path.unwrap_or_else(|| ".".to_string());
    let parsed = match parse_path(&path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("glob", Some(&path), &error, None),
    };
    let options = match parse_glob_options(args.max_results, args.include_hidden) {
        Ok(options) => options,
        Err(error) => {
            return result::failure(
                "glob",
                Some(parsed.normalized_path()),
                &error,
                Some("filesystem"),
            );
        }
    };

    execute_glob_on_path(parsed, &args.pattern, options, capability_domain_state)
}

fn execute_search(args_json: &str, capability_domain_state: &Value) -> ActionOutcome {
    let args = match parse_args::<SearchArgs>(args_json, "filesystem__search") {
        Ok(args) => args,
        Err(error) => return result::failure("search", None, &error, None),
    };
    let path = args.path.unwrap_or_else(|| ".".to_string());
    let parsed = match parse_path(&path) {
        Ok(parsed) => parsed,
        Err(error) => return result::failure("search", Some(&path), &error, None),
    };
    let options = match parse_search_options(args.include, args.max_results, args.case_sensitive) {
        Ok(options) => options,
        Err(error) => {
            return result::failure(
                "search",
                Some(parsed.normalized_path()),
                &error,
                Some("filesystem"),
            );
        }
    };

    execute_search_on_path(parsed, &args.pattern, options, capability_domain_state)
}

fn execute_list_on_path(
    path: ParsedPath,
    options: ListOptions,
    capability_domain_state: &Value,
) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::list(&path, options, capability_domain_state) {
        Ok(data) => result::success("list", &normalized_path, target, data),
        Err(error) => result::failure("list", Some(&normalized_path), &error, Some(target)),
    }
}

fn execute_read_on_path(
    path: ParsedPath,
    options: ReadOptions,
    capability_domain_state: &Value,
) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::read(&path, options, capability_domain_state) {
        Ok(data) => result::success("read", &normalized_path, target, data),
        Err(error) => result::failure("read", Some(&normalized_path), &error, Some(target)),
    }
}

fn execute_write_on_path(
    path: ParsedPath,
    content: &str,
    allow_override: bool,
    create_parents: bool,
    capability_domain_state: &Value,
) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::write(
        &path,
        content,
        allow_override,
        create_parents,
        capability_domain_state,
    ) {
        Ok(data) => result::success("write", &normalized_path, target, data),
        Err(error) => result::failure("write", Some(&normalized_path), &error, Some(target)),
    }
}

fn execute_replace_on_path(
    path: ParsedPath,
    old: &str,
    new: &str,
    mode: ReplaceMode,
    expected_replacements: Option<usize>,
    capability_domain_state: &Value,
) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::replace(
        &path,
        old,
        new,
        mode,
        expected_replacements,
        capability_domain_state,
    ) {
        Ok(data) => result::success("replace", &normalized_path, target, data),
        Err(error) => result::failure("replace", Some(&normalized_path), &error, Some(target)),
    }
}

fn execute_glob_on_path(
    path: ParsedPath,
    pattern: &str,
    options: GlobOptions,
    capability_domain_state: &Value,
) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::glob(&path, pattern, options, capability_domain_state) {
        Ok(data) => result::success("glob", &normalized_path, target, data),
        Err(error) => result::failure("glob", Some(&normalized_path), &error, Some(target)),
    }
}

fn execute_search_on_path(
    path: ParsedPath,
    pattern: &str,
    options: SearchOptions,
    capability_domain_state: &Value,
) -> ActionOutcome {
    let target = path.target_label();
    let normalized_path = path.normalized_path().to_string();

    match real::search(&path, pattern, options, capability_domain_state) {
        Ok(data) => result::success("search", &normalized_path, target, data),
        Err(error) => result::failure("search", Some(&normalized_path), &error, Some(target)),
    }
}

fn parse_list_options(args: ListArgs) -> Result<ListOptions, FsError> {
    let max_entries = parse_optional_usize(
        args.max_entries,
        "filesystem__list",
        "max_entries",
        1,
        LIST_MAX_ENTRIES_CAP,
    )?
    .unwrap_or(LIST_DEFAULT_MAX_ENTRIES);

    Ok(ListOptions {
        recursive: args.recursive.unwrap_or(false),
        max_entries,
        include_hidden: args.include_hidden.unwrap_or(false),
    })
}

fn parse_read_options(args: ReadArgs) -> Result<ReadOptions, FsError> {
    let offset_line = parse_optional_usize(
        args.offset_line,
        "filesystem__read",
        "offset_line",
        1,
        usize::MAX,
    )?
    .unwrap_or(READ_DEFAULT_OFFSET_LINE);

    let limit_lines = parse_optional_usize(
        args.limit_lines,
        "filesystem__read",
        "limit_lines",
        1,
        READ_MAX_LIMIT_LINES,
    )?
    .unwrap_or(READ_DEFAULT_LIMIT_LINES);

    Ok(ReadOptions {
        offset_line,
        limit_lines,
    })
}

fn parse_glob_options(
    max_results: Option<u64>,
    include_hidden: Option<bool>,
) -> Result<GlobOptions, FsError> {
    let max_results = parse_optional_usize(
        max_results,
        "filesystem__glob",
        "max_results",
        1,
        GLOB_MAX_RESULTS_CAP,
    )?
    .unwrap_or(GLOB_DEFAULT_MAX_RESULTS);

    Ok(GlobOptions {
        max_results,
        include_hidden: include_hidden.unwrap_or(false),
    })
}

fn parse_search_options(
    include: Option<Vec<String>>,
    max_results: Option<u64>,
    case_sensitive: Option<bool>,
) -> Result<SearchOptions, FsError> {
    let max_results = parse_optional_usize(
        max_results,
        "filesystem__search",
        "max_results",
        1,
        SEARCH_MAX_RESULTS_CAP,
    )?
    .unwrap_or(SEARCH_DEFAULT_MAX_RESULTS);

    Ok(SearchOptions {
        include: include.unwrap_or_default(),
        max_results,
        case_sensitive: case_sensitive.unwrap_or(false),
    })
}

fn parse_optional_usize(
    value: Option<u64>,
    action_id: &str,
    field: &str,
    min: usize,
    max: usize,
) -> Result<Option<usize>, FsError> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let converted = usize::try_from(raw).map_err(|_| {
        FsError::invalid_args(format!(
            "`{action_id}.{field}` value is too large for this platform"
        ))
    })?;
    if converted < min {
        return Err(FsError::invalid_args(format!(
            "`{action_id}.{field}` must be >= {min}"
        )));
    }
    if converted > max {
        return Err(FsError::invalid_args(format!(
            "`{action_id}.{field}` must be <= {max}"
        )));
    }
    Ok(Some(converted))
}

fn parse_args<T>(args_json: &str, action_id: &str) -> Result<T, FsError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(args_json).map_err(|error| {
        FsError::invalid_args(format!("failed to parse args for `{action_id}`: {error}"))
    })
}

#[cfg(test)]
mod tests;

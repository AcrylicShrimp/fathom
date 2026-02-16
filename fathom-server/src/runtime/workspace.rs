use std::path::PathBuf;

use anyhow::{Context, bail};

pub(super) fn canonicalize_workspace_root(workspace_root: PathBuf) -> anyhow::Result<PathBuf> {
    let workspace_root = if workspace_root.is_absolute() {
        workspace_root
    } else {
        std::env::current_dir()
            .context("failed to resolve current working directory")?
            .join(workspace_root)
    };

    let canonical = std::fs::canonicalize(&workspace_root).with_context(|| {
        format!(
            "failed to resolve workspace root `{}`",
            workspace_root.display()
        )
    })?;
    let metadata = std::fs::metadata(&canonical).with_context(|| {
        format!(
            "failed to read workspace root metadata `{}`",
            canonical.display()
        )
    })?;
    if !metadata.is_dir() {
        bail!(
            "workspace root must be a directory: `{}`",
            canonical.display()
        );
    }
    Ok(canonical)
}

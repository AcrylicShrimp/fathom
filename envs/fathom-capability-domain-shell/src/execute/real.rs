use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, timeout};

use super::error::ShellError;

#[derive(Debug, Clone)]
pub(crate) struct RealRunOptions {
    pub(crate) timeout_ms: u64,
    pub(crate) env_overrides: BTreeMap<String, String>,
    pub(crate) max_stdout_bytes: usize,
    pub(crate) max_stderr_bytes: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CapturedOutput {
    pub(crate) text: String,
    pub(crate) truncated_bytes: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandExecution {
    pub(crate) exit_code: Option<i32>,
    pub(crate) timed_out: bool,
    pub(crate) duration_ms: u64,
    pub(crate) stdout: CapturedOutput,
    pub(crate) stderr: CapturedOutput,
}

pub(crate) async fn run_command(
    command: &str,
    cwd: &Path,
    options: &RealRunOptions,
) -> Result<CommandExecution, ShellError> {
    let mut process = build_shell_command(command);
    process.current_dir(cwd);
    process.stdout(Stdio::piped());
    process.stderr(Stdio::piped());
    process.kill_on_drop(true);

    for (key, value) in &options.env_overrides {
        process.env(key, value);
    }

    let mut child = process
        .spawn()
        .map_err(|error| ShellError::spawn_failed(format!("failed to spawn command: {error}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ShellError::internal("failed to capture stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ShellError::internal("failed to capture stderr"))?;

    let stdout_task = tokio::spawn(capture_stream(stdout, options.max_stdout_bytes));
    let stderr_task = tokio::spawn(capture_stream(stderr, options.max_stderr_bytes));

    let start = Instant::now();
    let wait_result = timeout(Duration::from_millis(options.timeout_ms), child.wait()).await;
    let (timed_out, status) = match wait_result {
        Ok(waited) => (
            false,
            waited.map_err(|error| {
                ShellError::execution_failed(format!("failed to wait for command: {error}"))
            })?,
        ),
        Err(_) => {
            let _ = child.start_kill();
            let status = child.wait().await.map_err(|error| {
                ShellError::execution_failed(format!(
                    "failed to wait for command after timeout: {error}"
                ))
            })?;
            (true, status)
        }
    };

    let stdout = join_capture(stdout_task, "stdout").await?;
    let stderr = join_capture(stderr_task, "stderr").await?;

    Ok(CommandExecution {
        exit_code: status.code(),
        timed_out,
        duration_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        stdout,
        stderr,
    })
}

fn build_shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-lc").arg(command);
        cmd
    }
}

async fn join_capture(
    task: JoinHandle<Result<CapturedOutput, ShellError>>,
    stream_name: &str,
) -> Result<CapturedOutput, ShellError> {
    task.await.map_err(|error| {
        ShellError::internal(format!(
            "failed to join {stream_name} capture task: {error}"
        ))
    })?
}

async fn capture_stream<R>(mut reader: R, max_bytes: usize) -> Result<CapturedOutput, ShellError>
where
    R: AsyncRead + Unpin,
{
    let mut kept: Vec<u8> = Vec::new();
    let mut total_bytes = 0usize;
    let mut chunk = [0u8; 4096];

    loop {
        let read = reader.read(&mut chunk).await.map_err(|error| {
            ShellError::io_error(format!("failed to read process output stream: {error}"))
        })?;
        if read == 0 {
            break;
        }
        total_bytes += read;

        if kept.len() < max_bytes {
            let remaining = max_bytes - kept.len();
            let to_copy = remaining.min(read);
            kept.extend_from_slice(&chunk[..to_copy]);
        }
    }

    let truncated_bytes = total_bytes.saturating_sub(kept.len());
    let text = String::from_utf8_lossy(&kept).to_string();

    Ok(CapturedOutput {
        text,
        truncated_bytes,
    })
}

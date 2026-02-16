use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;

use serde_json::Value;
use tracing::warn;

#[derive(Clone)]
pub(crate) struct DiagnosticsSink {
    tx: Sender<WriteCommand>,
}

enum WriteCommand {
    AppendJsonl {
        relative_path: PathBuf,
        record: Value,
    },
    WriteJson {
        relative_path: PathBuf,
        payload: Value,
    },
}

impl DiagnosticsSink {
    pub(crate) fn new(root_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<WriteCommand>();
        let worker_root = root_dir.clone();
        thread::spawn(move || {
            while let Ok(command) = rx.recv() {
                match command {
                    WriteCommand::AppendJsonl {
                        relative_path,
                        record,
                    } => {
                        if let Err(error) =
                            append_jsonl_record(&worker_root, &relative_path, &record)
                        {
                            warn!(%error, ?relative_path, "failed to append diagnostics jsonl");
                        }
                    }
                    WriteCommand::WriteJson {
                        relative_path,
                        payload,
                    } => {
                        if let Err(error) = write_json_file(&worker_root, &relative_path, &payload)
                        {
                            warn!(%error, ?relative_path, "failed to write diagnostics json");
                        }
                    }
                }
            }
        });

        Self { tx }
    }

    pub(crate) fn append_session_record(&self, session_id: &str, record: Value) {
        let relative_path = PathBuf::from("sessions")
            .join(session_id)
            .join("events.jsonl");
        if self
            .tx
            .send(WriteCommand::AppendJsonl {
                relative_path,
                record,
            })
            .is_err()
        {
            warn!("diagnostics worker is unavailable (append_session_record)");
        }
    }

    pub(crate) fn write_invocation_context(
        &self,
        session_id: &str,
        invocation_seq: u64,
        payload: Value,
    ) {
        let relative_path = PathBuf::from("sessions")
            .join(session_id)
            .join("invocations")
            .join(format!("invocation-{invocation_seq}.json"));
        if self
            .tx
            .send(WriteCommand::WriteJson {
                relative_path,
                payload,
            })
            .is_err()
        {
            warn!("diagnostics worker is unavailable (write_invocation_context)");
        }
    }
}

fn append_jsonl_record(root: &Path, relative_path: &Path, record: &Value) -> anyhow::Result<()> {
    let target = root.join(relative_path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(target)?;
    file.write_all(serde_json::to_string(record)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn write_json_file(root: &Path, relative_path: &Path, payload: &Value) -> anyhow::Result<()> {
    let target = root.join(relative_path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(target)?;
    file.write_all(serde_json::to_string_pretty(payload)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::DiagnosticsSink;

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }

    #[test]
    fn diagnostics_sink_writes_session_jsonl_and_invocation_json() {
        let root = unique_temp_dir("fathom-diag");
        let sink = DiagnosticsSink::new(root.clone());

        sink.append_session_record(
            "session-1",
            json!({
                "event": "turn.started",
                "turn_id": 1,
            }),
        );
        sink.write_invocation_context(
            "session-1",
            1,
            json!({
                "event": "agent.invocation.context",
                "turn_id": 1,
            }),
        );

        let jsonl = root.join("sessions/session-1/events.jsonl");
        let invocation = root.join("sessions/session-1/invocations/invocation-1.json");

        for _ in 0..30 {
            if jsonl.exists() && invocation.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(jsonl.exists());
        assert!(invocation.exists());

        let events = std::fs::read_to_string(jsonl).expect("events file readable");
        assert!(events.contains("\"turn.started\""));

        let detail = std::fs::read_to_string(invocation).expect("invocation file readable");
        assert!(detail.contains("\"agent.invocation.context\""));
    }
}

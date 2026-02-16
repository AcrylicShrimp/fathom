use serde::Deserialize;
use serde_json::json;

use crate::fs::{self, TaskOutcome};
use crate::pb;
use crate::runtime::Runtime;
use crate::session::task_context::TaskExecutionContext;
use crate::system_tools;

#[derive(Debug, Deserialize)]
struct SendMessageArgs {
    content: String,
    #[serde(default)]
    user_id: String,
}

pub(crate) async fn execute_task_tool(
    runtime: &Runtime,
    context: &TaskExecutionContext,
    tool_name: &str,
    args_json: &str,
) -> Option<TaskOutcome> {
    match tool_name {
        "send_message" => Some(execute_send_message(args_json)),
        _ => {
            if let Some(outcome) =
                system_tools::execute_tool(runtime, context, tool_name, args_json).await
            {
                Some(outcome)
            } else {
                fs::execute_tool(runtime, tool_name, args_json).await
            }
        }
    }
}

pub(crate) fn should_enqueue_task_done_trigger(tool_name: &str) -> bool {
    !matches!(tool_name, "send_message")
}

pub(crate) fn extract_send_message_content(task: &pb::Task) -> Option<String> {
    if task.tool_name != "send_message" {
        return None;
    }
    if task.status != pb::TaskStatus::Succeeded as i32 {
        return None;
    }

    let args = parse_send_message_args(&task.args_json).ok()?;
    if args.content.trim().is_empty() {
        return None;
    }
    Some(args.content)
}

fn execute_send_message(args_json: &str) -> TaskOutcome {
    let args = match parse_send_message_args(args_json) {
        Ok(args) => args,
        Err(error) => {
            return TaskOutcome {
                succeeded: false,
                message: json!({
                    "ok": false,
                    "op": "send_message",
                    "error_code": "invalid_args",
                    "message": error,
                })
                .to_string(),
            };
        }
    };

    if args.content.trim().is_empty() {
        return TaskOutcome {
            succeeded: false,
            message: json!({
                "ok": false,
                "op": "send_message",
                "error_code": "invalid_args",
                "message": "send_message.content must be non-empty",
            })
            .to_string(),
        };
    }

    TaskOutcome {
        succeeded: true,
        message: json!({
            "ok": true,
            "op": "send_message",
            "content": args.content,
            "user_id": args.user_id,
        })
        .to_string(),
    }
}

fn parse_send_message_args(args_json: &str) -> Result<SendMessageArgs, String> {
    serde_json::from_str(args_json)
        .map_err(|error| format!("failed to parse send_message args: {error}"))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::pb;

    use super::{
        execute_send_message, extract_send_message_content, should_enqueue_task_done_trigger,
    };

    #[test]
    fn send_message_does_not_enqueue_task_done_trigger() {
        assert!(!should_enqueue_task_done_trigger("send_message"));
        assert!(should_enqueue_task_done_trigger("fs_read"));
        assert!(should_enqueue_task_done_trigger("memory_append"));
    }

    #[test]
    fn execute_send_message_returns_success_payload() {
        let outcome = execute_send_message(r#"{"content":"hello user"}"#);
        assert!(outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["op"], "send_message");
        assert_eq!(payload["content"], "hello user");
    }

    #[test]
    fn execute_send_message_rejects_empty_content() {
        let outcome = execute_send_message(r#"{"content":"   "}"#);
        assert!(!outcome.succeeded);

        let payload: Value = serde_json::from_str(&outcome.message).expect("valid json payload");
        assert_eq!(payload["ok"], false);
        assert_eq!(payload["error_code"], "invalid_args");
    }

    #[test]
    fn extract_send_message_content_reads_succeeded_task() {
        let task = pb::Task {
            task_id: "task-1".to_string(),
            session_id: "session-1".to_string(),
            tool_name: "send_message".to_string(),
            args_json: r#"{"content":"hello from task"}"#.to_string(),
            status: pb::TaskStatus::Succeeded as i32,
            result_message: String::new(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        };

        let content = extract_send_message_content(&task);
        assert_eq!(content.as_deref(), Some("hello from task"));
    }
}

use crate::pb;
use crate::util::now_unix_ms;

const STREAM_BATCH_WINDOW_MS: i64 = 40;

pub(super) struct TurnAssistantStreamEmitter {
    stream_id: String,
    pending_delta: String,
    full_content: String,
    last_flush_unix_ms: i64,
}

impl TurnAssistantStreamEmitter {
    pub(super) fn new(turn_id: u64) -> Self {
        Self {
            stream_id: format!("{turn_id}:assistant"),
            pending_delta: String::new(),
            full_content: String::new(),
            last_flush_unix_ms: 0,
        }
    }

    pub(super) fn on_assistant_text_delta<F>(&mut self, delta: &str, mut emit: F)
    where
        F: FnMut(pb::session_event::Kind),
    {
        if delta.is_empty() {
            return;
        }

        self.pending_delta.push_str(delta);
        self.full_content.push_str(delta);
        self.flush_if_due(false, &mut emit);
    }

    pub(super) fn on_assistant_text_done<F>(
        &mut self,
        final_text: Option<&str>,
        mut emit: F,
    ) -> String
    where
        F: FnMut(pb::session_event::Kind),
    {
        if let Some(final_text) = final_text
            && !final_text.is_empty()
            && !self.full_content.ends_with(final_text)
        {
            if final_text.starts_with(&self.full_content) {
                let suffix = &final_text[self.full_content.len()..];
                if !suffix.is_empty() {
                    self.pending_delta.push_str(suffix);
                    self.full_content.push_str(suffix);
                }
            } else {
                self.pending_delta.push_str(final_text);
                self.full_content = final_text.to_string();
            }
        }

        self.flush_if_due(true, &mut emit);
        self.full_content.clone()
    }

    pub(super) fn stream_id(&self) -> String {
        self.stream_id.clone()
    }

    fn flush_if_due<F>(&mut self, force_done: bool, emit: &mut F)
    where
        F: FnMut(pb::session_event::Kind),
    {
        let now = now_unix_ms();
        let should_flush = force_done || now - self.last_flush_unix_ms >= STREAM_BATCH_WINDOW_MS;
        if !should_flush {
            return;
        }

        if !self.pending_delta.is_empty() {
            let delta = std::mem::take(&mut self.pending_delta);
            emit(pb::session_event::Kind::AssistantStream(
                pb::AssistantStreamEvent {
                    stream_id: self.stream_id.clone(),
                    delta,
                    done: false,
                    created_at_unix_ms: now,
                    user_id: String::new(),
                },
            ));
        }

        self.last_flush_unix_ms = now;

        if force_done {
            emit(pb::session_event::Kind::AssistantStream(
                pb::AssistantStreamEvent {
                    stream_id: self.stream_id.clone(),
                    delta: String::new(),
                    done: true,
                    created_at_unix_ms: now_unix_ms(),
                    user_id: String::new(),
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TurnAssistantStreamEmitter;
    use crate::pb;

    #[test]
    fn emitter_streams_assistant_delta_and_done() {
        let mut emitter = TurnAssistantStreamEmitter::new(3);
        let mut events = Vec::new();

        emitter.on_assistant_text_delta("hel", |event| events.push(event));
        emitter.on_assistant_text_delta("lo", |event| events.push(event));
        let output = emitter.on_assistant_text_done(None, |event| events.push(event));

        assert_eq!(output, "hello");
        assert!(events.iter().any(|event| matches!(
            event,
            pb::session_event::Kind::AssistantStream(pb::AssistantStreamEvent {
                done: true,
                stream_id,
                ..
            }) if stream_id == "3:assistant"
        )));
    }
}

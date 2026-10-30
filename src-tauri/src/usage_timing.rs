//! Codex session timing, adapted from CC Switch. See data/README.md for attribution.
use serde_json::Value;

#[derive(Default)]
pub(crate) struct RequestTimer {
    boundary: Option<i64>,
    previous_count: Option<i64>,
    start: Option<i64>,
    output: Option<i64>,
    tool_after_output: Option<i64>,
    usage_record: Option<(i64, Value)>,
    first_output: Option<i64>,
}

pub(crate) fn timestamp_ms(value: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value.get("timestamp")?.as_str()?)
        .ok()
        .map(|time| time.timestamp_millis())
}

impl RequestTimer {
    pub(crate) fn observe(&mut self, value: &Value) {
        let Some(at) = timestamp_ms(value) else {
            return;
        };
        let payload = &value["payload"];
        let kind = value["type"].as_str().unwrap_or_default();
        let item = payload["type"].as_str().unwrap_or_default();
        match kind {
            "turn_context" => self.boundary = self.boundary.max(Some(at)),
            "response_item" => {
                let output = item == "reasoning"
                    || item.ends_with("_call")
                    || (item == "message" && payload["role"] == "assistant");
                if output {
                    // Old versions flush the preceding response after token_count.
                    if self.start.is_none()
                        && self.previous_count.is_some_and(|prev| at - prev <= 100)
                    {
                        return;
                    }
                    if self.start.is_none() {
                        self.start = self.boundary;
                    }
                    self.output = Some(at);
                    self.tool_after_output = None;
                } else if item == "message" || item.ends_with("_output") {
                    self.boundary = self.boundary.max(Some(at));
                    if item.ends_with("_output") && self.output.is_some() {
                        self.tool_after_output = Some(at);
                    }
                }
            }
            "token_usage_record" => {
                self.usage_record = payload
                    .get("usage")
                    .filter(|usage| usage.is_object())
                    .cloned()
                    .map(|usage| (at, usage));
            }
            "event_msg" if item == "task_started" || item == "turn_aborted" => {
                *self = Self {
                    boundary: self.boundary.max(Some(at)),
                    previous_count: self.previous_count,
                    ..Self::default()
                };
            }
            "event_msg" if item == "item_completed" => {
                if matches!(
                    payload["item"]["type"].as_str(),
                    Some("Reasoning" | "AgentMessage")
                ) {
                    if let Some(started) = payload["started_at_ms"].as_i64() {
                        self.first_output =
                            Some(self.first_output.map_or(started, |old| old.min(started)));
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn finish(&mut self, at: Option<i64>, last: Option<&Value>) -> (u64, Option<u64>) {
        let start = self.start.or(self.boundary);
        let first = self.first_output;
        let record_end = self
            .usage_record
            .take()
            .filter(|(_, usage)| {
                last.is_none_or(|last| {
                    ["input_tokens", "cached_input_tokens", "output_tokens"]
                        .iter()
                        .all(|key| {
                            usage[*key].as_u64().unwrap_or_default()
                                == last[*key].as_u64().unwrap_or_default()
                        })
                })
            })
            .map(|(at, _)| at);
        let waited_for_tools = self
            .tool_after_output
            .is_some_and(|tool| at.is_none_or(|at| at - tool <= 100));
        let end = record_end.or(if waited_for_tools { self.output } else { at });
        *self = Self {
            boundary: self.boundary.max(at),
            previous_count: at,
            ..Self::default()
        };
        let duration = start
            .zip(end)
            .and_then(|(start, end)| end.checked_sub(start))
            .filter(|ms| (100..=3_600_000).contains(ms))
            .unwrap_or_default() as u64;
        let first_ms = first
            .zip(start)
            .and_then(|(first, start)| first.checked_sub(start))
            .filter(|ms| *ms >= 0 && (*ms as u64) < duration)
            .map(|ms| ms as u64);
        (duration, first_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn event(kind: &str, at: &str, payload: Value) -> Value {
        json!({"type":kind,"timestamp":format!("2026-10-30T00:00:{at}Z"),"payload":payload})
    }
    #[test]
    fn excludes_tool_execution_and_resets_aborted_requests() {
        let mut timer = RequestTimer::default();
        timer.observe(&event("turn_context", "00", json!({})));
        timer.observe(&event(
            "response_item",
            "10",
            json!({"type":"function_call"}),
        ));
        timer.observe(&event(
            "response_item",
            "40",
            json!({"type":"function_call_output"}),
        ));
        assert_eq!(
            timer
                .finish(
                    Some(timestamp_ms(&event("event_msg", "40", json!({}))).unwrap()),
                    None
                )
                .0,
            10_000
        );
        timer.observe(&event("event_msg", "42", json!({"type":"turn_aborted"})));
        timer.observe(&event("event_msg", "50", json!({"type":"task_started"})));
        assert_eq!(
            timer
                .finish(timestamp_ms(&event("event_msg", "55", json!({}))), None)
                .0,
            5_000
        );
    }
    #[test]
    fn uses_matching_usage_record_and_rejects_invalid_windows() {
        let mut timer = RequestTimer::default();
        timer.observe(&event("turn_context", "00", json!({})));
        timer.observe(&event(
            "token_usage_record",
            "12",
            json!({"usage":{"output_tokens":300}}),
        ));
        assert_eq!(
            timer
                .finish(
                    timestamp_ms(&event("event_msg", "25", json!({}))),
                    Some(&json!({"output_tokens":300}))
                )
                .0,
            12_000
        );
        assert_eq!(timer.finish(None, None), (0, None));
    }

    #[test]
    fn ignores_late_response_flush_and_tracks_first_output_without_reusing_usage() {
        let mut timer = RequestTimer::default();
        timer.observe(&event("turn_context", "00", json!({})));
        timer.observe(&event(
            "response_item",
            "10",
            json!({"type":"message","role":"assistant"}),
        ));
        timer.finish(timestamp_ms(&event("event_msg", "10", json!({}))), None);
        timer.observe(&event(
            "response_item",
            "10.050",
            json!({"type":"message","role":"assistant"}),
        ));
        timer.observe(&event(
            "response_item",
            "11",
            json!({"type":"function_call_output"}),
        ));
        timer.observe(&event(
            "response_item",
            "14",
            json!({"type":"message","role":"assistant"}),
        ));
        assert_eq!(
            timer.finish(timestamp_ms(&event("event_msg", "14", json!({}))), None),
            (3000, None)
        );

        timer.observe(&event("turn_context", "20", json!({})));
        let first = timestamp_ms(&event("event_msg", "22", json!({}))).unwrap();
        timer.observe(&event(
            "event_msg",
            "24",
            json!({"type":"item_completed","item":{"type":"Reasoning"},"started_at_ms":first}),
        ));
        timer.observe(&event(
            "token_usage_record",
            "25",
            json!({"usage":{"output_tokens":300}}),
        ));
        assert_eq!(
            timer.finish(
                timestamp_ms(&event("event_msg", "30", json!({}))),
                Some(&json!({"output_tokens":301})),
            ),
            (10_000, Some(2000))
        );
    }
}

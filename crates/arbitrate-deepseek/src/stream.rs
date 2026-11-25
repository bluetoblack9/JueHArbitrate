#[cfg(test)]
use crate::wire::{ChatRequest, StreamChunk, StreamOptions, decode_stream_chunk};
use arbitrate_core::types::{Message, ToolCall, Usage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    ReasoningDelta(String),
    Delta(String),
    ToolCallDelta(ToolCall),
    Done { usage: Usage, finish_reason: String },
}

/// Prepare a copy of the history for DeepSeek, retaining reasoning for tool calls.
pub fn strip_reasoning_content(messages: &[Message]) -> Vec<Message> {
    let mut prepared = messages.to_vec();
    for message in &mut prepared {
        message.predicted_next.clear();
        if message.role != "assistant" || message.tool_calls.is_empty() {
            message.reasoning_content.clear();
        }
    }
    prepared
}

#[cfg(test)]
fn prepare_stream_request(request: &ChatRequest) -> ChatRequest {
    let mut prepared = request.clone();
    prepared.stream = true;
    prepared.stream_options.get_or_insert(StreamOptions {
        include_usage: true,
    });
    prepared
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{ChatRequest, EncodeError, StreamOptions, encode_chat};
    use arbitrate_core::types::{ImagePart, Message};

    #[test]
    fn prepare_sets_stream_and_defaults_usage_on_a_copy() {
        for (options, include_usage) in [
            (None, true),
            (
                Some(StreamOptions {
                    include_usage: false,
                }),
                false,
            ),
        ] {
            let request = ChatRequest {
                stream_options: options,
                ..Default::default()
            };
            let original = encode_chat(&request).unwrap();
            let prepared = encode_chat(&prepare_stream_request(&request)).unwrap();
            assert_eq!(
                prepared,
                encode_chat(&prepare_stream_request(&request)).unwrap()
            );
            let sent: serde_json::Value = serde_json::from_slice(&prepared).unwrap();
            assert_eq!(sent["stream"].as_bool(), Some(true));
            assert_eq!(
                sent["stream_options"]["include_usage"].as_bool(),
                Some(include_usage)
            );
            assert_eq!(encode_chat(&request).unwrap(), original);
        }

        let request = ChatRequest {
            messages: vec![Message {
                images: vec![ImagePart::default()],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(matches!(
            encode_chat(&prepare_stream_request(&request)),
            Err(EncodeError::UnsupportedAttachments)
        ));
    }
}

#[cfg(test)]
const MAX_LINE_BYTES: usize = 1024 * 1024;

#[cfg(test)]
#[derive(Default)]
struct SseDecoder {
    partial: Vec<u8>,
}

#[cfg(test)]
enum SseFrame {
    Skip,
    Done,
    Chunk(StreamChunk),
}

#[cfg(test)]
enum StreamFailure {
    Decode(String),
    Read(String),
}

#[cfg(test)]
impl SseDecoder {
    fn next_frame(&mut self, input: &mut &[u8]) -> Result<Option<SseFrame>, StreamFailure> {
        let newline = input.iter().position(|byte| *byte == b'\n');
        let part = &input[..newline.unwrap_or(input.len())];
        if part.len() > MAX_LINE_BYTES - self.partial.len() {
            return Err(StreamFailure::Read("SSE line exceeds 1 MiB".into()));
        }

        let Some(newline) = newline else {
            self.partial.extend_from_slice(part);
            *input = &[];
            return Ok(None);
        };
        *input = &input[newline + 1..];

        if self.partial.is_empty() {
            return Self::decode_line(part).map(Some);
        }
        self.partial.extend_from_slice(part);
        let frame = Self::decode_line(&self.partial);
        self.partial.clear();
        frame.map(Some)
    }

    fn finish(self) -> Result<SseFrame, StreamFailure> {
        Self::decode_line(&self.partial)
    }

    fn decode_line(line: &[u8]) -> Result<SseFrame, StreamFailure> {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(payload) = line.strip_prefix(b"data:") else {
            return Ok(SseFrame::Skip);
        };
        let payload = std::str::from_utf8(payload)
            .map_err(|error| StreamFailure::Decode(error.to_string()))?
            .trim();
        match payload {
            "" => Ok(SseFrame::Skip),
            "[DONE]" => Ok(SseFrame::Done),
            _ => decode_stream_chunk(payload.as_bytes())
                .map(SseFrame::Chunk)
                .map_err(|error| StreamFailure::Decode(error.to_string())),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
struct ReadOutcome {
    emitted_any: bool,
    usage: Usage,
    finish_reason: String,
    failure: Option<StreamFailure>,
}

#[cfg(test)]
impl ReadOutcome {
    fn apply_chunk(&mut self, chunk: StreamChunk) -> Vec<StreamEvent> {
        if let Some(usage) = chunk.usage {
            self.usage = usage;
        }
        let mut events = Vec::new();
        for choice in chunk.choices {
            if !choice.reasoning_content.is_empty() {
                events.push(StreamEvent::ReasoningDelta(choice.reasoning_content));
            }
            if !choice.content.is_empty() {
                events.push(StreamEvent::Delta(choice.content));
            }
            events.extend(
                choice
                    .tool_calls
                    .into_iter()
                    .map(StreamEvent::ToolCallDelta),
            );
            if !choice.finish_reason.is_empty() {
                self.finish_reason = choice.finish_reason;
            }
        }
        events
    }

    fn terminal(self, open_error: Option<&str>) -> StreamEvent {
        let finish_reason = match (open_error, self.failure) {
            (Some(message), _) => format!("stream_error:{}", truncate_message(message)),
            (None, Some(StreamFailure::Read(message))) => {
                format!("stream_error:{}", truncate_message(&message))
            }
            (None, Some(StreamFailure::Decode(message))) => {
                format!("decode_error:{}", truncate_message(&message))
            }
            (None, None) => self.finish_reason,
        };
        StreamEvent::Done {
            usage: self.usage,
            finish_reason,
        }
    }
}

#[cfg(test)]
fn truncate_message(message: &str) -> std::borrow::Cow<'_, str> {
    const MAX_ERROR_BYTES: usize = 128;
    if message.len() <= MAX_ERROR_BYTES {
        return message.into();
    }
    let mut end = MAX_ERROR_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &message[..end]).into()
}

#[cfg(test)]
mod framing_tests {
    use super::*;

    fn read_script(
        chunks: &[&[u8]],
        transport_error: Option<&str>,
    ) -> (ReadOutcome, Vec<StreamEvent>) {
        let mut decoder = SseDecoder::default();
        let mut outcome = ReadOutcome::default();
        let mut events = Vec::new();
        let mut absorb = |frame| match frame {
            Ok(SseFrame::Skip) => true,
            Ok(SseFrame::Done) => false,
            Ok(SseFrame::Chunk(chunk)) => {
                events.extend(outcome.apply_chunk(chunk));
                outcome.emitted_any = !events.is_empty();
                true
            }
            Err(failure) => {
                outcome.failure = Some(failure);
                false
            }
        };

        for mut bytes in chunks.iter().copied() {
            loop {
                match decoder.next_frame(&mut bytes) {
                    Ok(None) => break,
                    result => {
                        if !absorb(result.map(Option::unwrap)) {
                            return (outcome, events);
                        }
                    }
                }
            }
        }
        if let Some(message) = transport_error {
            outcome.failure = Some(StreamFailure::Read(message.into()));
        } else {
            absorb(decoder.finish());
        }
        (outcome, events)
    }

    #[test]
    fn decodes_fragmented_and_coalesced_lines_until_done() {
        let first = "ta: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\r".as_bytes();
        let split = first.iter().position(|byte| *byte >= 128).unwrap() + 1;
        let (outcome, events) = read_script(&[
            b"\n: comment\nevent: ignored\n data: broken\ndata:  \n",
            b"da",
            &first[..split],
            &first[split..],
            b"\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"!\",\"reasoning_content\":\"r\"}}]}\n\ndata: [DONE]\ndata: bad\n",
        ], None);
        assert_eq!(
            events,
            [
                StreamEvent::Delta("你好".into()),
                StreamEvent::ReasoningDelta("r".into()),
                StreamEvent::Delta("!".into()),
            ]
        );
        assert!(outcome.emitted_any);
        assert!(outcome.failure.is_none());
    }

    #[test]
    fn parses_unterminated_line_only_at_clean_eof() {
        let last: &[u8] = br#"data: {"choices":[{"delta":{"content":"last"}}]}"#;
        let (clean, events) = read_script(&[last], None);
        assert!(clean.failure.is_none());
        assert_eq!(events, [StreamEvent::Delta("last".into())]);

        let (broken, events) = read_script(&[last], Some("reset"));
        assert!(events.is_empty());
        assert_eq!(
            broken.terminal(None),
            StreamEvent::Done {
                usage: Usage::default(),
                finish_reason: "stream_error:reset".into()
            }
        );
    }

    #[test]
    fn rejects_bad_utf8_even_inside_ignored_fields() {
        let (outcome, events) = read_script(
            &[
                b"data: {\"usage\":{\"prompt_tokens\":1}}\n",
                b"data: {\"ignored\":\"\xff\",\"choices\":[],\"usage\":{\"prompt_tokens\":99}}\n\n",
            ],
            None,
        );
        assert!(events.is_empty());
        assert!(matches!(outcome.failure, Some(StreamFailure::Decode(_))));
        assert!(matches!(outcome.terminal(None),
            StreamEvent::Done { usage, finish_reason } if usage.prompt_tokens == 1 && finish_reason.starts_with("decode_error:")));
    }

    #[test]
    fn maps_each_choice_in_order_and_keeps_latest_usage_and_nonempty_finish() {
        let (outcome, events) = read_script(&[
            b"data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"r\",\"content\":\"c\",\"tool_calls\":[{\"index\":0},{\"index\":1}]},\"finish_reason\":\"length\"},{\"delta\":{\"content\":\"next\"}}],\"usage\":{\"total_tokens\":9}}\n",
            b"data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n",
            b"data: {\"choices\":[{\"finish_reason\":\"\"}],\"usage\":null}\n",
            b"data: {\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3,\"total_tokens\":8,\"prompt_cache_hit_tokens\":4,\"prompt_cache_miss_tokens\":1}}\n",
        ], None);
        assert_eq!(
            events,
            [
                StreamEvent::ReasoningDelta("r".into()),
                StreamEvent::Delta("c".into()),
                StreamEvent::ToolCallDelta(ToolCall {
                    index: Some(0),
                    ..Default::default()
                }),
                StreamEvent::ToolCallDelta(ToolCall {
                    index: Some(1),
                    ..Default::default()
                }),
                StreamEvent::Delta("next".into()),
            ]
        );
        assert_eq!(
            outcome.terminal(None),
            StreamEvent::Done {
                usage: Usage {
                    prompt_tokens: 5,
                    completion_tokens: 3,
                    total_tokens: 8,
                    prompt_cache_hit_tokens: 4,
                    prompt_cache_miss_tokens: 1
                },
                finish_reason: "stop".into(),
            }
        );

        let (zero, _) = read_script(
            &[b"data: {\"usage\":{\"total_tokens\":5}}\ndata: {\"usage\":{}}\n"],
            None,
        );
        assert_eq!(zero.usage, Usage::default());
    }

    #[test]
    fn enforces_raw_line_byte_limit_with_lf_outside_budget() {
        let mut line = b"data: {}".to_vec();
        line.resize(MAX_LINE_BYTES, b' ');
        let (exact, _) = read_script(&[&line[..256], &line[256..], b"\n"], None);
        assert!(exact.failure.is_none());

        for ending in [b" ".as_slice(), b" \n".as_slice(), b"\r\n".as_slice()] {
            let (oversize, _) = read_script(&[&line, ending], None);
            assert!(matches!(oversize.failure, Some(StreamFailure::Read(_))));
        }
    }

    #[test]
    fn terminal_truncates_utf8_safely_and_prioritizes_open_failure() {
        let exact = "a".repeat(128);
        let longer = format!("{exact}x");
        let bisected = format!("{}中x", "a".repeat(127));
        let boundary = format!("{}中", "a".repeat(125));
        for (message, expected) in [
            (exact.clone(), exact.clone()),
            (longer, format!("{exact}…")),
            (bisected, format!("{}…", "a".repeat(127))),
            (format!("{boundary}x"), format!("{boundary}…")),
        ] {
            let outcome = ReadOutcome {
                finish_reason: "stop".into(),
                failure: Some(StreamFailure::Decode(message)),
                ..Default::default()
            };
            assert_eq!(
                outcome.terminal(None),
                StreamEvent::Done {
                    usage: Usage::default(),
                    finish_reason: format!("decode_error:{expected}"),
                }
            );
        }
        let outcome = ReadOutcome {
            failure: Some(StreamFailure::Decode("first stream failed".into())),
            ..Default::default()
        };
        assert_eq!(
            outcome.terminal(Some("reopen failed")),
            StreamEvent::Done {
                usage: Usage::default(),
                finish_reason: "stream_error:reopen failed".into(),
            }
        );
    }
}

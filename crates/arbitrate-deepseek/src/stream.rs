#[cfg(test)]
use crate::wire::{ChatRequest, StreamOptions};
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

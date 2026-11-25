use arbitrate_core::types::{ChatResponse, Choice, FilePart, ImagePart, Message, ToolCall, Usage};

pub fn sample_turn() -> Vec<Message> {
    vec![
        Message {
            role: "user".into(),
            content: "资料".into(),
            images: vec![
                ImagePart {
                    asset: "one.png".into(),
                    ..ImagePart::default()
                },
                ImagePart {
                    asset: "two.png".into(),
                    ..ImagePart::default()
                },
            ],
            files: vec![FilePart {
                asset: "doc.pdf".into(),
                ..FilePart::default()
            }],
            ..Message::default()
        },
        Message {
            role: "assistant".into(),
            reasoning_content: "inspect the document".into(),
            predicted_next: "summarize".into(),
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                kind: "function".into(),
                name: "read_file".into(),
                arguments: r#"{"path":"a"}"#.into(),
                index: None,
            }],
            ..Message::default()
        },
        Message {
            role: "tool".into(),
            name: "read_file".into(),
            tool_call_id: "call-1".into(),
            content: "document text".into(),
            ..Message::default()
        },
    ]
}

pub fn sample_response() -> ChatResponse {
    ChatResponse {
        id: "test-chat-1".into(),
        object: "chat.completion".into(),
        created: 1_700_000_000,
        model: "deepseek-flash".into(),
        choices: vec![
            Choice {
                index: 0,
                message: Message {
                    role: "assistant".into(),
                    content: "done".into(),
                    ..Message::default()
                },
                finish_reason: "stop".into(),
            },
            Choice {
                index: 1,
                message: sample_turn().remove(1),
                finish_reason: "tool_calls".into(),
            },
        ],
        usage: Usage {
            prompt_tokens: 10,
            completion_tokens: 3,
            total_tokens: 13,
            prompt_cache_hit_tokens: 8,
            prompt_cache_miss_tokens: 2,
        },
    }
}

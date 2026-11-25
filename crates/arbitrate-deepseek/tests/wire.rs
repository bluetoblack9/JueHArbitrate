#[path = "../../../tests/support/provider_messages.rs"]
mod provider_messages;

use arbitrate_core::types::{Message, ToolCall, Usage};
use arbitrate_deepseek::wire::*;
use serde_json::{Value, json};

#[test]
fn minimal_request_has_stable_wire_shape() {
    let mut input = ChatRequest {
        model: MODEL_V41_FLASH.into(),
        messages: vec![Message {
            role: "user".into(),
            content: "hi".into(),
            ..Message::default()
        }],
        ..ChatRequest::default()
    };
    let expected = br#"{"model":"deepseek-flash","messages":[{"role":"user","content":"hi"}]}"#;
    assert_eq!(encode_chat(&input).unwrap(), expected);
    assert_eq!(encode_chat(&input).unwrap(), expected);
    input.messages[0].content.clear();
    assert_eq!(
        encode_chat(&input).unwrap(),
        br#"{"model":"deepseek-flash","messages":[{"role":"user"}]}"#
    );
    input.messages.clear();
    assert_eq!(
        encode_chat(&input).unwrap(),
        br#"{"model":"deepseek-flash","messages":[]}"#
    );
}

#[test]
fn request_preserves_explicit_parameters_and_wire_field_names() {
    let input = ChatRequest {
        model: MODEL_V41_FLASH.into(),
        stream: true,
        temperature: Some(0.0),
        top_p: Some(0.4),
        max_tokens: 32,
        tools: vec![Tool {
            kind: "function".into(),
            function: ToolFunction {
                name: "lookup".into(),
                description: "Look up an item".into(),
                parameters: Some(json!({"type":"object"})),
            },
        }],
        tool_choice: Some(json!({"type":"function","function":{"name":"lookup"}})),
        response_format: Some(ResponseFormat {
            kind: "json_object".into(),
        }),
        stream_options: Some(StreamOptions {
            include_usage: false,
        }),
        thinking: Some(ThinkingMode {
            kind: "enabled".into(),
        }),
        reasoning_effort: "high".into(),
        safety_check: Some(SafetyCheck {
            level: SAFETY_CHECK_HIGH.into(),
        }),
        ..ChatRequest::default()
    };
    let wire: Value = serde_json::from_slice(&encode_chat(&input).unwrap()).unwrap();
    assert_eq!(
        wire,
        json!({
            "model":"deepseek-flash","messages":[], "stream":true,"temperature":0.0,"top_p":0.4,"max_tokens":32,
            "tools":[{"type":"function","function":{"name":"lookup","description":"Look up an item","parameters":{"type":"object"}}}],
            "tool_choice":{"type":"function","function":{"name":"lookup"}},
            "response_format":{"type":"json_object"},"stream_options":{"include_usage":false},
            "thinking":{"type":"enabled"},"reasoning_effort":"high","safety_check":{"level":"high"}
        })
    );
}

#[test]
fn history_tools_omit_stream_indexes_and_predictions() {
    let mut input = ChatRequest {
        messages: provider_messages::sample_turn(),
        ..ChatRequest::default()
    };
    input.messages[0].images.clear();
    input.messages[0].files.clear();
    let assistant = &mut input.messages[1];
    assistant.tool_calls[0].index = Some(0);
    assistant.tool_calls.push(ToolCall {
        index: Some(1),
        id: "call-2".into(),
        ..assistant.tool_calls[0].clone()
    });
    let original = input.messages.clone();
    let wire: Value = serde_json::from_slice(&encode_chat(&input).unwrap()).unwrap();
    let messages = wire["messages"].as_array().unwrap();
    assert_eq!(
        messages[1],
        json!({
            "role":"assistant","reasoning_content":"inspect the document","tool_calls":[
                {"id":"call-1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a\"}"}},
                {"id":"call-2","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a\"}"}}
            ]
        })
    );
    assert_eq!(
        messages[2],
        json!({"role":"tool","content":"document text","name":"read_file","tool_call_id":"call-1"})
    );
    assert_eq!(input.messages, original);
}

#[test]
fn rejects_attachments_and_non_finite_fields_locally() {
    let original = provider_messages::sample_turn();
    let request = ChatRequest {
        messages: original.clone(),
        ..ChatRequest::default()
    };
    assert!(matches!(
        encode_chat(&request),
        Err(EncodeError::UnsupportedAttachments)
    ));
    assert_eq!(request.messages, original);
    let request = ChatRequest {
        top_p: Some(f64::NAN),
        ..ChatRequest::default()
    };
    assert!(matches!(
        encode_chat(&request),
        Err(EncodeError::NonFinite("top_p"))
    ));
    let request = ChatRequest {
        temperature: Some(f64::INFINITY),
        ..ChatRequest::default()
    };
    assert!(matches!(
        encode_chat(&request),
        Err(EncodeError::NonFinite("temperature"))
    ));
}

#[test]
fn decodes_response_with_shared_domain_shape_and_cache_usage() {
    let mut expected = provider_messages::sample_response();
    expected.choices[1].message.predicted_next.clear();
    expected.choices[1].message.tool_calls[0].index = Some(0);
    let response = decode_chat(br#"{
        "id":"test-chat-1","object":"chat.completion","created":1700000000,"model":"deepseek-flash",
        "choices":[
            {"index":0,"message":{"role":"assistant","content":"done"},"finish_reason":"stop"},
            {"index":1,"message":{"role":"assistant","content":null,"reasoning_content":"inspect the document",
              "tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a\"}"}}]},
              "finish_reason":"tool_calls"}
        ],
        "usage":{"prompt_tokens":10,"completion_tokens":3,"total_tokens":13,"prompt_cache_hit_tokens":8,"prompt_cache_miss_tokens":2}
    }"#).unwrap();
    assert_eq!(response, expected);
    assert_eq!(format_hit_ratio(&response.usage), "80.0%");
}

#[test]
fn defaults_optional_response_fields_and_rejects_invalid_types() {
    for raw in [b"{}".as_slice(), br#"{"usage":null}"#] {
        assert_eq!(decode_chat(raw).unwrap().usage, Usage::default());
    }
    let response = decode_chat(
        br#"{"choices":[{"message":{"tool_calls":[{"function":{"name":"demo"}}]}}],
        "usage":{"prompt_tokens":4}}"#,
    )
    .unwrap();
    assert_eq!(
        response.usage,
        Usage {
            prompt_tokens: 4,
            ..Usage::default()
        }
    );
    assert_eq!(response.choices[0].message.tool_calls[0].index, None);
    for invalid in [
        b"{".as_slice(),
        b"[]",
        b"null",
        br#"{"usage":{"prompt_tokens":"4"}}"#,
        br#"{"choices":[{"message":{"content":9}}]}"#,
    ] {
        assert!(decode_chat(invalid).is_err(), "{:?}", invalid);
    }
}

#[test]
fn only_the_current_flash_id_enables_thinking() {
    assert!(should_enable_thinking(MODEL_V41_FLASH));
    for model in [
        "",
        "deepseek-v4-pro",
        "deepseek-v4.1-flash",
        "DEEPSEEK-FLASH",
        "deepseek-flash-latest",
    ] {
        assert!(!should_enable_thinking(model), "{model}");
    }
}

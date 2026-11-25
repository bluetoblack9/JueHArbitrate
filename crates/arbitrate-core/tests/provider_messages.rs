#[path = "../../../tests/support/provider_messages.rs"]
mod provider_messages;

#[test]
fn tool_result_keeps_association_and_history_owns_attachments() {
    let history = provider_messages::sample_turn();
    assert_eq!(history[0].content, "资料");
    assert_eq!(history[0].images.len(), 2);
    assert_eq!(history[1].tool_calls[0].id, history[2].tool_call_id);
    assert_eq!(history[1].tool_calls[0].name, history[2].name);
    assert_eq!(history[1].tool_calls[0].arguments, r#"{"path":"a"}"#);
    assert_eq!(history[1].reasoning_content, "inspect the document");
    assert_eq!(history[1].predicted_next, "summarize");

    let mut copy = history.clone();
    copy[0].images[0].url = "https://example.invalid/one.png".into();
    let file = &mut copy[0].files[0];
    file.file_id = "file-123".into();
    file.expires_at = 1_700_000_100;
    file.purpose = "text_recognition".into();
    assert_eq!(file.asset, "doc.pdf");
    assert_eq!(file.expires_at, 1_700_000_100);
    assert_eq!(file.purpose, "text_recognition");
    assert!(history[0].images[0].url.is_empty());
    assert!(history[0].files[0].file_id.is_empty());
    assert_eq!(history[0].files[0].asset, "doc.pdf");
}

#[test]
fn response_keeps_alternatives_and_existing_usage() {
    let response = provider_messages::sample_response();
    assert_eq!(response.id, "test-chat-1");
    assert_eq!(response.object, "chat.completion");
    assert_eq!(response.created, 1_700_000_000);
    assert_eq!(response.model, "deepseek-flash");
    assert_eq!(response.choices.len(), 2);
    assert_eq!(response.choices[0].index, 0);
    assert_eq!(response.choices[0].message.content, "done");
    assert_eq!(response.choices[0].finish_reason, "stop");
    assert_eq!(response.choices[1].index, 1);
    assert_eq!(
        response.choices[1].message,
        provider_messages::sample_turn()[1]
    );
    assert_eq!(response.choices[1].finish_reason, "tool_calls");
    assert_eq!(response.usage.total_tokens, 13);
    assert_eq!(response.usage.hit_ratio(), 0.8);
}

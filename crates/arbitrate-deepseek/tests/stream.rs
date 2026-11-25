#[allow(dead_code)]
#[path = "../../../tests/support/provider_messages.rs"]
mod provider_messages;

use arbitrate_core::types::Message;
use arbitrate_deepseek::strip_reasoning_content;

#[test]
fn strip_preserves_only_tool_call_reasoning_and_keeps_input() {
    let sample = provider_messages::sample_turn();
    let mut with_calls = sample[1].clone();
    with_calls.tool_calls.push(with_calls.tool_calls[0].clone());
    let mut history = vec![with_calls];
    for role in ["assistant", "user", "system", "tool"] {
        history.push(Message {
            role: role.into(),
            reasoning_content: format!("reasoning-{role}"),
            predicted_next: format!("prediction-{role}"),
            ..Message::default()
        });
    }
    history[4].tool_call_id = sample[2].tool_call_id.clone();
    let original = history.clone();
    let prepared = strip_reasoning_content(&history);

    assert_eq!(history, original);
    assert_eq!(prepared[0].reasoning_content, original[0].reasoning_content);
    assert_eq!(prepared[0].tool_calls, original[0].tool_calls);
    assert_eq!(prepared[4].tool_call_id, prepared[0].tool_calls[0].id);
    assert!(prepared[1..].iter().all(|m| m.reasoning_content.is_empty()));
    assert!(prepared.iter().all(|m| m.predicted_next.is_empty()));
}

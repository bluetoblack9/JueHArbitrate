mod support;
use std::collections::BTreeSet;

const KINDS: [&str; 15] = [
    "AgentStart",
    "AgentEnd",
    "TurnStart",
    "TurnEnd",
    "MessageStart",
    "MessageDelta",
    "MessageEnd",
    "ToolExecStart",
    "ToolDelta",
    "ToolExecEnd",
    "ErrorEvent",
    "PlanProposalApproved",
    "PlanProposalAdjustRequested",
    "PlanProposalCancelled",
    "PlanStepUpdated",
];

fn required_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "TurnStart" | "TurnEnd" => &["index"],
        "MessageDelta" => &["delta", "reasoning"],
        "ToolExecStart" | "ToolExecEnd" => &["call_id", "name"],
        "PlanProposalApproved" => &["steps"],
        "PlanStepUpdated" => &["steps", "current_idx"],
        _ => &[],
    }
}

#[test]
fn four_scenario_fixtures_valid_shape() {
    let known: BTreeSet<&str> = KINDS.into_iter().collect();
    for name in ["plain-text", "tool-call", "permission-denied", "plan"] {
        let text = support::load_fixture(&format!("events/{name}.jsonl"));
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(!lines.is_empty());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(lines[0]).unwrap()["kind"],
            "AgentStart"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(lines.last().unwrap()).unwrap()["kind"],
            "AgentEnd"
        );
        for l in lines {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            let kind = v["kind"].as_str().unwrap();
            assert!(known.contains(kind), "未知事件 kind: {kind}");
            for f in required_fields(kind) {
                assert!(v["fields"].get(f).is_some(), "{name}: {kind} 缺字段 {f}");
            }
        }
    }
}

#[test]
fn permission_denial_is_result_not_error() {
    let text = support::load_fixture("events/permission-denied.jsonl");
    let end: serde_json::Value = text
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["kind"] == "ToolExecEnd")
        .unwrap();
    assert!(end["fields"]["err"].is_null());
    assert!(
        end["fields"]["result"]
            .as_str()
            .unwrap()
            .contains("permission denied")
    );
}

//! DeepSeek chat request and response encoding.

use std::fmt;
use std::marker::PhantomData;

use arbitrate_core::types::{ChatResponse, Choice, Message, ToolCall, Usage};
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

pub use arbitrate_core::types::format_hit_ratio;

pub const MODEL_V41_FLASH: &str = "deepseek-flash";
pub const SAFETY_CHECK_HIGH: &str = "high";

pub fn should_enable_thinking(model: &str) -> bool {
    model == MODEL_V41_FLASH
}

#[derive(Clone, Debug, Default)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_tokens: i64,
    pub tools: Vec<Tool>,
    pub tool_choice: Option<Value>,
    pub response_format: Option<ResponseFormat>,
    pub stream_options: Option<StreamOptions>,
    pub thinking: Option<ThinkingMode>,
    pub reasoning_effort: String,
    pub safety_check: Option<SafetyCheck>,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub kind: String,
    pub function: ToolFunction,
}

#[derive(Clone, Debug)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct ResponseFormat {
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Clone, Debug)]
pub struct ThinkingMode {
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct SafetyCheck {
    pub level: String,
}

#[derive(Debug)]
pub enum EncodeError {
    UnsupportedAttachments,
    NonFinite(&'static str),
    Json(serde_json::Error),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedAttachments => f.write_str("chat attachments are not supported yet"),
            Self::NonFinite(field) => write!(f, "{field} must be finite"),
            Self::Json(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for EncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct OutboundRequest<'a> {
    model: &'a str,
    messages: Vec<OutboundMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OutboundTool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<Kind<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<UsageOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<Kind<'a>>,
    #[serde(skip_serializing_if = "str::is_empty")]
    reasoning_effort: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    safety_check: Option<SafetyLevel<'a>>,
}

#[derive(Serialize)]
struct Kind<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
}

#[derive(Serialize)]
struct UsageOption {
    include_usage: bool,
}

#[derive(Serialize)]
struct SafetyLevel<'a> {
    level: &'a str,
}

#[derive(Serialize)]
struct OutboundTool<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    function: OutboundDefinition<'a>,
}

#[derive(Serialize)]
struct OutboundDefinition<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    description: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<&'a Value>,
}

#[derive(Serialize)]
struct OutboundMessage<'a> {
    role: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    content: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    name: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    tool_call_id: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OutboundCall<'a>>,
    #[serde(skip_serializing_if = "str::is_empty")]
    reasoning_content: &'a str,
}

#[derive(Serialize)]
struct OutboundCall<'a> {
    #[serde(skip_serializing_if = "str::is_empty")]
    id: &'a str,
    #[serde(rename = "type", skip_serializing_if = "str::is_empty")]
    kind: &'a str,
    function: OutboundFunction<'a>,
}

#[derive(Serialize)]
struct OutboundFunction<'a> {
    name: &'a str,
    arguments: &'a str,
}

impl<'a> TryFrom<&'a Message> for OutboundMessage<'a> {
    type Error = EncodeError;

    fn try_from(message: &'a Message) -> Result<Self, Self::Error> {
        if !message.images.is_empty() || !message.files.is_empty() {
            return Err(EncodeError::UnsupportedAttachments);
        }
        Ok(Self {
            role: &message.role,
            content: &message.content,
            name: &message.name,
            tool_call_id: &message.tool_call_id,
            tool_calls: message
                .tool_calls
                .iter()
                .map(|call| OutboundCall {
                    id: &call.id,
                    kind: &call.kind,
                    function: OutboundFunction {
                        name: &call.name,
                        arguments: &call.arguments,
                    },
                })
                .collect(),
            reasoning_content: &message.reasoning_content,
        })
    }
}

pub fn encode_chat(request: &ChatRequest) -> Result<Vec<u8>, EncodeError> {
    for (name, value) in [
        ("temperature", request.temperature),
        ("top_p", request.top_p),
    ] {
        if value.is_some_and(|number| !number.is_finite()) {
            return Err(EncodeError::NonFinite(name));
        }
    }

    let outbound = OutboundRequest {
        model: &request.model,
        messages: request
            .messages
            .iter()
            .map(OutboundMessage::try_from)
            .collect::<Result<_, _>>()?,
        stream: request.stream.then_some(true),
        temperature: request.temperature,
        top_p: request.top_p,
        max_tokens: (request.max_tokens != 0).then_some(request.max_tokens),
        tools: request
            .tools
            .iter()
            .map(|tool| OutboundTool {
                kind: &tool.kind,
                function: OutboundDefinition {
                    name: &tool.function.name,
                    description: &tool.function.description,
                    parameters: tool.function.parameters.as_ref(),
                },
            })
            .collect(),
        tool_choice: request.tool_choice.as_ref(),
        response_format: request
            .response_format
            .as_ref()
            .map(|value| Kind { kind: &value.kind }),
        stream_options: request.stream_options.as_ref().map(|value| UsageOption {
            include_usage: value.include_usage,
        }),
        thinking: request
            .thinking
            .as_ref()
            .map(|value| Kind { kind: &value.kind }),
        reasoning_effort: &request.reasoning_effort,
        safety_check: request.safety_check.as_ref().map(|value| SafetyLevel {
            level: &value.level,
        }),
    };
    serde_json::to_vec(&outbound).map_err(EncodeError::Json)
}

// Serde also accepts positional arrays for derived structs. The wire protocol
// uses JSON objects for messages, choices, and usage counters.
#[derive(Default)]
pub(crate) struct WireObject<T>(pub(crate) T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for WireObject<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = WireObject<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a DeepSeek JSON object")
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                T::deserialize(MapAccessDeserializer::new(map)).map(WireObject)
            }
        }

        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

#[derive(Deserialize)]
struct InboundResponse {
    id: Option<String>,
    object: Option<String>,
    created: Option<i64>,
    model: Option<String>,
    choices: Option<Vec<WireObject<InboundChoice>>>,
    usage: Option<WireObject<InboundUsage>>,
}

#[derive(Deserialize)]
struct InboundChoice {
    index: Option<usize>,
    message: Option<WireObject<InboundMessage>>,
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct InboundMessage {
    role: Option<String>,
    content: Option<String>,
    name: Option<String>,
    tool_call_id: Option<String>,
    tool_calls: Option<Vec<WireObject<InboundCall>>>,
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct InboundCall {
    index: Option<usize>,
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    function: Option<WireObject<InboundFunction>>,
}

#[derive(Default, Deserialize)]
struct InboundFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Default, Deserialize)]
struct InboundUsage {
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
    prompt_cache_hit_tokens: Option<i64>,
    prompt_cache_miss_tokens: Option<i64>,
}

impl From<InboundMessage> for Message {
    fn from(message: InboundMessage) -> Self {
        Self {
            role: message.role.unwrap_or_default(),
            content: message.content.unwrap_or_default(),
            name: message.name.unwrap_or_default(),
            tool_call_id: message.tool_call_id.unwrap_or_default(),
            tool_calls: message
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(|WireObject(call)| {
                    let WireObject(function) = call.function.unwrap_or_default();
                    ToolCall {
                        index: call.index,
                        id: call.id.unwrap_or_default(),
                        kind: call.kind.unwrap_or_default(),
                        name: function.name.unwrap_or_default(),
                        arguments: function.arguments.unwrap_or_default(),
                    }
                })
                .collect(),
            reasoning_content: message.reasoning_content.unwrap_or_default(),
            ..Self::default()
        }
    }
}

impl From<InboundUsage> for Usage {
    fn from(usage: InboundUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens.unwrap_or_default(),
            completion_tokens: usage.completion_tokens.unwrap_or_default(),
            total_tokens: usage.total_tokens.unwrap_or_default(),
            prompt_cache_hit_tokens: usage.prompt_cache_hit_tokens.unwrap_or_default(),
            prompt_cache_miss_tokens: usage.prompt_cache_miss_tokens.unwrap_or_default(),
        }
    }
}

pub fn decode_chat(bytes: &[u8]) -> Result<ChatResponse, serde_json::Error> {
    let WireObject(response) = serde_json::from_slice::<WireObject<InboundResponse>>(bytes)?;
    Ok(ChatResponse {
        id: response.id.unwrap_or_default(),
        object: response.object.unwrap_or_default(),
        created: response.created.unwrap_or_default(),
        model: response.model.unwrap_or_default(),
        choices: response
            .choices
            .unwrap_or_default()
            .into_iter()
            .map(|WireObject(choice)| Choice {
                index: choice.index.unwrap_or_default(),
                message: choice.message.unwrap_or_default().0.into(),
                finish_reason: choice.finish_reason.unwrap_or_default(),
            })
            .collect(),
        usage: response.usage.unwrap_or_default().0.into(),
    })
}

use std::{collections::HashMap, time::Duration};

use super::model::{
    ChatMessageRequest, Message, ModelResponse, ToolCallRequestFunction, ToolCallRequestMessage,
    UserChatMessageRequest,
};
use crate::{
    agent::openrouter::model::{FinishReason, ResponseUsage, ToolCall},
    models::{
        model::{ChatMessageResponse, LLMModel, ModelResponseErr},
        thread::Thread,
        tool::Tool,
    },
    tool::{
        ToolCallRequest, bash_tool, create_file_tool, edit_file_tool, fd_tool, list_files_tool,
        read_tool, rg_tool,
    },
};
use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::broadcast;

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/";

pub enum OpenRouterModel {
    DeepSeekV4Pro,
    DeepSeekV4Flash,
    DeepSeekR1,

    MoonShotAiKIMI27,
}

impl OpenRouterModel {
    fn to_str(&self) -> &str {
        match self {
            OpenRouterModel::DeepSeekV4Pro => "deepseek/deepseek-v4-pro",
            OpenRouterModel::DeepSeekV4Flash => "deepseek/deepseek-v4-flash",

            OpenRouterModel::DeepSeekR1 => "deepseek/deepseek-r1",
            OpenRouterModel::MoonShotAiKIMI27 => "moonshotai/kimi-k2.7-code",
        }
    }
}

pub struct OpenRouterBase {
    model_type: OpenRouterModel,
    api_key: String,
}

impl OpenRouterBase {
    pub fn new(model_type: OpenRouterModel, api_key: String) -> Self {
        OpenRouterBase {
            model_type,
            api_key,
        }
    }
}

#[derive(Default)]
struct ResponseContext {
    is_delta: bool,
    aggregated: ChatMessageResponse,
    new_message: ChatMessageResponse,
    tool_call_aggregator: HashMap<usize, ToolCall>,
    finish_reason: Option<FinishReason>,
    usage: Option<ResponseUsage>,
}

#[async_trait]
impl LLMModel for OpenRouterBase {
    async fn generate(
        &self,
        prompt: &Thread,
        resp_tx: broadcast::Sender<ChatMessageResponse>,
        tools: Vec<Tool>,
    ) {
        let msg = {
            let mut msg: UserChatMessageRequest = prompt.into();
            msg.model = self.model_type.to_str().to_string();
            if !tools.is_empty() {
                msg.tools = Some(tools);
            }
            msg
        };
        tracing::info!(
            "Sending request to OpenRouter API with model: {}",
            msg.model
        );
        let response_res = reqwest::Client::new()
            .post(format!("{}{}", OPENROUTER_API_URL, "chat/completions"))
            .timeout(Duration::from_secs(120))
            .bearer_auth(self.api_key.clone())
            .json::<UserChatMessageRequest>(&msg)
            .send()
            .await;

        let response = match response_res {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("Failed to send request to OpenRouter API: {}", e);
                resp_tx
                    .send(ChatMessageResponse {
                        role: "assistant".to_string(),
                        done: true,
                        error: Some(ModelResponseErr::RequestErr(e.to_string())),
                        ..Default::default()
                    })
                    .unwrap();
                return;
            }
        };

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut response_context: ResponseContext = ResponseContext::default();

        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else {
                send_error(
                    resp_tx.clone(),
                    ModelResponseErr::RequestErr("stream read failed".to_string()),
                );
                return;
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            loop {
                match drain_next_event(&mut buffer) {
                    DrainResult::Done => {
                        response_context.finish_reason = Some(FinishReason::Stop);
                        conclude_request(resp_tx.clone(), response_context);
                        return;
                    }
                    DrainResult::Incomplete => break,
                    DrainResult::Skipped => continue,
                    DrainResult::Json(json) => {
                        tracing::debug!("Received JSON: {}", json);

                        match serde_json::from_str::<ModelResponse>(&json) {
                            Ok(event) => {
                                if let Some(e) = event.error {
                                    send_error(resp_tx.clone(), ModelResponseErr::RequestErr(e));
                                    return;
                                }
                                if let Err(e) = process_model_response(event, &mut response_context)
                                {
                                    send_error(resp_tx.clone(), e);
                                    return;
                                }

                                merge_context(&mut response_context);
                                if has_streamable_content(&response_context.new_message) {
                                    let _ = resp_tx.send(response_context.new_message.clone());
                                }
                                if response_context.finish_reason.is_some() {
                                    conclude_request(resp_tx.clone(), response_context);
                                    return;
                                }
                            }
                            Err(e) => {
                                if json.contains("OPENROUTER") {
                                    tracing::warn!("OpenRouter metadata: {}", json);
                                    continue;
                                }
                                tracing::error!(
                                    "Failed to parse complete JSON object: {}. Error: {}",
                                    json,
                                    e
                                );
                                send_error(
                                    resp_tx.clone(),
                                    ModelResponseErr::ParseErr(e.to_string()),
                                );
                                return;
                            }
                        }
                    }
                }
            }
        }
        send_error(resp_tx, ModelResponseErr::NotEndedRequest)
    }

    fn name(&self) -> &str {
        self.model_type.to_str()
    }

    fn version(&self) -> &str {
        todo!()
    }
}

fn merge_context(context: &mut ResponseContext) {
    if let Some(c) = &mut context.aggregated.content {
        if let Some(c_new) = &context.new_message.content {
            c.push_str(c_new);
        }
    } else {
        context.aggregated.content = context.new_message.content.clone()
    }
    if let Some(t) = &mut context.aggregated.thinking {
        if let Some(t_new) = &context.new_message.thinking {
            t.push_str(t_new);
        }
    } else {
        context.aggregated.thinking = context.new_message.thinking.clone();
    }
}

fn has_streamable_content(msg: &ChatMessageResponse) -> bool {
    msg.content.as_ref().is_some_and(|c| !c.is_empty())
        || msg.thinking.as_ref().is_some_and(|t| !t.is_empty())
        || msg.tool_calls.is_some()
}

enum DrainResult {
    Done,
    Incomplete,
    Skipped,
    Json(String),
}

/// Pull the next SSE event from the buffer.
///
/// OpenRouter (and some upstream providers) may split a single JSON object across
/// TCP chunks. Newlines inside the stream are not reliable JSON boundaries, so we
/// extract complete `{...}` objects by brace matching instead of splitting on `\n`.
fn drain_next_event(buffer: &mut String) -> DrainResult {
    loop {
        drain_prefix_whitespace(buffer);
        if buffer.is_empty() {
            return DrainResult::Incomplete;
        }

        if buffer.starts_with(':') {
            let Some(pos) = buffer.find('\n') else {
                return DrainResult::Incomplete;
            };
            buffer.drain(..=pos);
            continue;
        }

        if buffer.starts_with("data:") {
            buffer.drain(..5);
            continue;
        }

        if buffer.starts_with("[DONE]") {
            buffer.clear();
            return DrainResult::Done;
        }

        if !buffer.starts_with('{') {
            let Some(pos) = buffer.find('\n') else {
                return DrainResult::Incomplete;
            };
            buffer.drain(..=pos);
            return DrainResult::Skipped;
        }

        let json_len = match complete_json_end(buffer) {
            Some(len) => len,
            None => return DrainResult::Incomplete,
        };

        let json: String = buffer.drain(..json_len).collect();
        drain_prefix_whitespace(buffer);
        return DrainResult::Json(json);
    }
}

fn drain_prefix_whitespace(buffer: &mut String) {
    let skip = buffer.len() - buffer.trim_start().len();
    if skip > 0 {
        buffer.drain(..skip);
    }
}

/// Returns the byte length of a complete top-level JSON object starting at `{`.
fn complete_json_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'{') {
        return None;
    }

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;

    for (i, &b) in bytes.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        match b {
            b'\\' if in_string => escape = true,
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn send_error(tx: broadcast::Sender<ChatMessageResponse>, err: ModelResponseErr) {
    tx.send(ChatMessageResponse {
        done: true,
        error: Some(err),
        ..Default::default()
    })
    .unwrap();
}

fn process_model_response(
    event: ModelResponse,
    context: &mut ResponseContext,
) -> Result<(), ModelResponseErr> {
    if event.choices.is_empty() {
        return Err(ModelResponseErr::NoChoiceErr);
    }
    let choice = event.choices[0].clone();

    let role = String::from("assistant");
    let mut content: Option<String> = None;
    let mut thinking: Option<String> = None;
    let done = false;

    let message: Message;

    if let Some(msg) = choice.message {
        message = msg
    } else if let Some(delta) = choice.delta {
        message = delta;
        context.is_delta = true
    } else {
        return Err(ModelResponseErr::ParseErr(
            "no message and no delta".to_string(),
        ));
    }

    if let Some(tool_calls) = message.tool_calls.filter(|tc| !tc.is_empty()) {
        process_tool_call(&tool_calls, &mut context.tool_call_aggregator);
    }
    if let Some(c) = message.content.filter(|c| !c.is_empty()) {
        content = Some(c);
    } else if let Some(t) = message.thinking.filter(|t| !t.is_empty()) {
        thinking = Some(t);
    } else if let Some(t) = message.reasoning.filter(|t| !t.is_empty()) {
        thinking = Some(t);
    }

    context.new_message = ChatMessageResponse {
        role,
        content,
        thinking,
        done,
        ..Default::default()
    };
    if !context.is_delta && !context.tool_call_aggregator.is_empty() {
        context.new_message.tool_calls =
            Some(into_tool_call(context.tool_call_aggregator.clone()).unwrap());
    }
    context.finish_reason = choice.finish_reason;
    // Usage typically arrives on the final chunk alongside finish_reason.
    if event.usage.is_some() {
        context.usage = event.usage;
    }
    Ok(())
}

fn into_tool_call(context: HashMap<usize, ToolCall>) -> Result<Vec<ToolCallRequest>, String> {
    let mut res = Vec::new();
    for (_, t) in context.iter() {
        res.push(t.clone().try_into().map_err(|e| format!("{:?}", e))?)
    }
    Ok(res)
}

fn process_tool_call(resp: &[ToolCall], aggregator: &mut HashMap<usize, ToolCall>) {
    for tool in resp.iter() {
        let Some(idx) = tool.index else { continue };
        if let Some(agg) = aggregator.get_mut(&idx) {
            if let Some(ref tool_name) = tool.function.name {
                if let Some(ref mut agg_tool_name) = agg.function.name {
                    agg_tool_name.push_str(tool_name)
                } else {
                    agg.function.name = Some(tool_name.clone())
                }
            }
            if let Some(ref tool_argument) = tool.function.arguments {
                if let Some(ref mut agg_tool_arg) = agg.function.arguments {
                    agg_tool_arg.push_str(tool_argument)
                } else {
                    agg.function.arguments = Some(tool_argument.clone())
                }
            }
            if let Some(ref tool_type) = tool.tool_type {
                if let Some(ref mut agg_tool_type) = agg.tool_type {
                    agg_tool_type.push_str(tool_type)
                } else {
                    agg.tool_type = Some(tool_type.clone())
                }
            }
        } else {
            aggregator.insert(idx, tool.clone());
        }
    }
}

fn conclude_request(tx: broadcast::Sender<ChatMessageResponse>, mut ctx: ResponseContext) {
    if !ctx.tool_call_aggregator.is_empty() {
        let tt = into_tool_call(ctx.tool_call_aggregator);
        match tt {
            Err(e) => {
                tx.send(ChatMessageResponse {
                    error: Some(ModelResponseErr::ToolCallErr(e)),
                    ..Default::default()
                })
                .unwrap();
                return;
            }
            Ok(r) => ctx.aggregated.tool_calls = Some(r),
        }
    }
    ctx.aggregated.done = true;
    ctx.aggregated.usage = ctx.usage.map(|u| crate::models::model::UsageInfo {
        prompt_tokens: Some(u.prompt_tokens as u64),
        completion_tokens: Some(u.completion_tokens as u64),
        total_tokens: Some(u.total_tokens as u64),
        cost: u.cost,
    });
    tx.send(ctx.aggregated).unwrap();
}

impl From<&Thread> for UserChatMessageRequest {
    fn from(val: &Thread) -> Self {
        let result = UserChatMessageRequest {
            model: String::new(), // This will be set later in the generate function
            messages: val
                .get_context()
                .iter()
                .flat_map(|m| {
                    let mut data = vec![];
                    if let Some(content) = &m.content {
                        data.push(ChatMessageRequest {
                            role: m.role.to_string(),
                            content: Some(content.clone()),
                            ..Default::default()
                        })
                    };
                    if m.response.is_some() {
                        data.push(ChatMessageRequest {
                            role: "assistant".to_string(),
                            content: m.response.clone(),
                            ..Default::default()
                        })
                    }
                    if let Some(resp) = &m.tool_response {
                        data.push(ChatMessageRequest {
                            role: "tool".to_string(),
                            tool_call_id: Some(resp.id.clone()),
                            content: Some(resp.result.clone()),
                            ..Default::default()
                        })
                    }
                    if let Some(tools) = &m.tools {
                        data.push(ChatMessageRequest {
                            role: "assistant".to_string(),
                            tool_calls: Some(
                                tools
                                    .iter()
                                    .map(|t| ToolCallRequestMessage {
                                        id: t.id.clone(),
                                        tool_type: t.tool_type.clone(),
                                        function: ToolCallRequestFunction {
                                            index: t.function.index,
                                            name: t.function.name.clone(),
                                            arguments: t.function.arguments.clone(),
                                        },
                                        error: None,
                                    })
                                    .collect(),
                            ),
                            ..Default::default()
                        })
                    }
                    data
                })
                .collect(),
            response_format: None,
            tools: None,
            stream: true,
            keep_alive: None,
            temperature: Some(1.0),
            format: None,
        };
        tracing::info!(
            "Converted thread into UserChatMessageRequest: {:?}",
            serde_json::to_string_pretty(&result).unwrap()
        );
        result
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::{
        super::model::{ToolCall, ToolCallFunction},
        DrainResult, complete_json_end, drain_next_event, process_tool_call,
    };

    #[test]
    fn complete_json_end_finds_balanced_object() {
        let s = r#"{"choices":[{"delta":{"arguments":"\"path\""}}]}"#;
        assert_eq!(complete_json_end(s), Some(s.len()));
    }

    #[test]
    fn complete_json_end_returns_none_for_truncated_object() {
        let s = r#"{"choices":[{"delta":{"arguments":"\"path\""#;
        assert_eq!(complete_json_end(s), None);
    }

    #[test]
    fn drain_next_event_waits_for_complete_json_across_chunks() {
        let chunk1 = concat!(
            "data: ",
            r#"{"id":"gen-1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"content":null,"role":"assistant","tool_calls":[{"index":0,"function":{"arguments":"\"path\""#,
        );
        let chunk2 = concat!(r#"me/bab"}}]},"finish_reason":null}]}"#, "\n\n",);

        let mut buffer = chunk1.to_string();
        assert!(matches!(
            drain_next_event(&mut buffer),
            DrainResult::Incomplete
        ));

        buffer.push_str(chunk2);
        match drain_next_event(&mut buffer) {
            DrainResult::Json(json) => {
                assert!(json.contains(r#""arguments":"\"path\"me/bab""#));
                serde_json::from_str::<serde_json::Value>(&json).expect("valid json");
            }
            other => panic!("expected Json, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn drain_next_event_handles_done() {
        let mut buffer = "data: [DONE]\n\n".to_string();
        assert!(matches!(drain_next_event(&mut buffer), DrainResult::Done));
    }

    #[test]
    fn test_process_tool_call() {
        struct Assertions {
            name: String,
            arguments: Option<String>,
            index: usize,
        }
        struct Testcase {
            times_called: usize,
            req: Vec<Vec<ToolCall>>,
            assertions: Vec<Assertions>,
        }
        let tool_call: Vec<Vec<ToolCall>> = vec![
            vec![tool_object(0, Some(String::from("a")), None)],
            vec![tool_object(0, Some(String::from("b")), None)],
            vec![tool_object(0, Some(String::from("c")), None)],
            vec![tool_object(
                0,
                Some(String::from("b")),
                Some(String::from("b")),
            )],
            vec![tool_object(0, None, Some(String::from("g")))],
        ];

        let tool_call_2: Vec<Vec<ToolCall>> = vec![
            vec![
                tool_object(1, Some(String::from("a")), None),
                tool_object(2, Some(String::from("b")), None),
            ],
            vec![tool_object(1, Some(String::from("c")), None)],
            vec![tool_object(
                2,
                Some(String::from("b")),
                Some(String::from("b")),
            )],
            vec![tool_object(2, None, Some(String::from("g")))],
        ];
        let tcs: Vec<Testcase> = vec![
            Testcase {
                times_called: tool_call.len(),
                req: tool_call,
                assertions: vec![Assertions {
                    name: "abcb".to_string(),
                    arguments: Some("bg".to_string()),
                    index: 0,
                }],
            },
            Testcase {
                times_called: tool_call_2.len(),
                req: tool_call_2,
                assertions: vec![
                    Assertions {
                        index: 1,
                        name: "ac".to_string(),
                        arguments: None,
                    },
                    Assertions {
                        index: 2,
                        name: "bb".to_string(),
                        arguments: Some("bg".to_string()),
                    },
                ],
            },
        ];
        for tc in tcs.iter() {
            let mut agg: HashMap<usize, ToolCall> = HashMap::new();
            for i in 0..tc.times_called {
                process_tool_call(tc.req.get(i).unwrap(), &mut agg);
            }
            for assert in &tc.assertions {
                match agg.get(&assert.index) {
                    Some(a) => {
                        assert_eq!(Some(assert.name.clone()), a.function.name);
                        assert_eq!(assert.arguments.clone(), a.function.arguments);
                    }
                    None => panic!("no item with the assertion index found"),
                }
            }
        }
    }
    fn tool_object(index: usize, name: Option<String>, arguments: Option<String>) -> ToolCall {
        ToolCall {
            id: None,
            index: Some(index),
            function: ToolCallFunction { name, arguments },
            tool_type: Some(String::from("function")),
        }
    }
}

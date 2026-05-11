use super::model::{
    ChatMessageRequest, ToolCallRequestFunction, ToolCallRequestMessage, UserChatMessageRequest,
};
use super::model::{Message, ModelResponse};
use crate::{
    models::{
        model::{ChatMessageResponse, LLMModel},
        thread::Thread,
    },
    tool::{create_file_tool, edit_file_tool, fd_tool, list_files_tool, read_tool, rg_tool},
};
use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::broadcast;

pub enum GroqModel {
    GptOss120B,
    Groq2,
    Groq3,
}

impl GroqModel {
    fn to_str(&self) -> &str {
        match self {
            GroqModel::GptOss120B => "openai/gpt-oss-120b",
            GroqModel::Groq2 => "groq2",
            GroqModel::Groq3 => "groq3",
        }
    }
}

pub struct GroqBase {
    model_type: GroqModel,
    api_key: String,
}

impl GroqBase {
    pub fn new(model_type: GroqModel, api_key: String) -> Self {
        GroqBase {
            model_type,
            api_key,
        }
    }

    pub fn process(&self, input: &str) -> String {
        // Placeholder for actual processing logic
        format!("Processed: {}", input)
    }
}

#[async_trait]
impl LLMModel for GroqBase {
    async fn generate(&self, prompt: &Thread, resp_tx: broadcast::Sender<ChatMessageResponse>) {
        let msg = {
            let mut msg: UserChatMessageRequest = prompt.into();
            msg.model = self.model_type.to_str().to_string();
            msg
        };
        let response = reqwest::Client::new()
            .post("https://api.groq.com/openai/v1/chat/completions")
            .bearer_auth(self.api_key.clone())
            .json::<UserChatMessageRequest>(&msg)
            .send()
            .await
            .unwrap();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut complete_response = String::new();
        let mut done = false;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // parse complete lines
            while let Some(pos) = buffer.find('\n') {
                let mut line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if line == "data: [DONE]" {
                    done = true;
                    break;
                }

                line = line
                    .split(": ")
                    .nth(1)
                    .map(|s| s.to_string())
                    .unwrap_or(line.clone());

                tracing::debug!("Received line: {}", line);

                match serde_json::from_str::<ModelResponse>(&line) {
                    Ok(event) => {
                        if event.choices.is_empty() {
                            continue;
                        }
                        let choice = event.choices[0].clone();

                        if let Some(reason) = &choice.finish_reason {
                            tracing::info!("Finish reason: {:?}", reason);
                            done = true;
                        }

                        let message: Message;

                        if let Some(msg) = choice.message {
                            message = msg
                        } else if let Some(delta) = choice.delta {
                            message = delta
                        } else {
                            continue;
                        }

                        if let Some(tool_calls) = message.tool_calls {
                            resp_tx
                                .send(ChatMessageResponse {
                                    role: "assistant".to_string(),
                                    tool_calls: Some(
                                        tool_calls
                                            .into_iter()
                                            .map(|tc| tc.clone().into())
                                            .collect(),
                                    ),
                                    done: false,
                                    ..Default::default()
                                })
                                .unwrap();
                        }

                        if let Some(content) = message.content {
                            let to_push = content.clone();
                            resp_tx
                                .send(ChatMessageResponse {
                                    role: "assistant".to_string(),
                                    content: Some(content.clone()),
                                    done: false,
                                    ..Default::default()
                                })
                                .unwrap();
                            complete_response.push_str(&to_push);
                        } else if let Some(thinking) = message.thinking {
                            resp_tx
                                .send(ChatMessageResponse {
                                    role: "assistant".to_string(),
                                    thinking: Some(thinking.clone()),
                                    done: false,
                                    ..Default::default()
                                })
                                .unwrap();
                        } else if let Some(thinking) = message.reasoning {
                            resp_tx
                                .send(ChatMessageResponse {
                                    role: "assistant".to_string(),
                                    thinking: Some(thinking.clone()),
                                    done: false,
                                    ..Default::default()
                                })
                                .unwrap();
                            complete_response.push_str(&format!("Thinking: {}", thinking));
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse line as JSON: {}. Error: {}", line, e);
                    }
                }
                if !complete_response.is_empty() && done {
                    resp_tx
                        .send(ChatMessageResponse {
                            role: "assistant".to_string(),
                            content: Some(complete_response.clone()),
                            done: true,
                            ..Default::default()
                        })
                        .unwrap();
                }
            }
        }
    }

    fn name(&self) -> &str {
        self.model_type.to_str()
    }

    fn version(&self) -> &str {
        todo!()
    }
}

impl Into<UserChatMessageRequest> for &Thread {
    fn into(self) -> UserChatMessageRequest {
        let result = UserChatMessageRequest {
            model: "gemma4:e4b".to_string(),
            messages: self
                .get_context()
                .iter()
                .map(|m| {
                    let mut data = vec![];
                    if let Some(content) = &m.content {
                        data.push(ChatMessageRequest {
                            role: m.role.to_string(),
                            content: Some(content.clone()),
                            ..Default::default()
                        })
                    };
                    if !m.response.is_none() {
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
                                    .into_iter()
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
                .flatten()
                .collect(),
            response_format: None,
            tools: Some(vec![
                read_tool(),
                list_files_tool(),
                create_file_tool(),
                edit_file_tool(),
                fd_tool(),
                rg_tool(),
            ]),
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

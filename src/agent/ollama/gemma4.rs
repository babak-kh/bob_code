use super::model::{ChatMessageRequest, UserChatMessageRequest};
use crate::models::model::ChatMessageResponse;
use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::broadcast;

use crate::{
    models::{model::LLMModel, thread::Thread},
    tool::ToolCallRequest,
};

pub struct GEMMA4Model;

#[async_trait]
impl LLMModel for GEMMA4Model {
    fn name(&self) -> &str {
        "gemma4"
    }

    fn version(&self) -> &str {
        todo!()
    }

    async fn generate(&self, prompt: &Thread, resp_tx: broadcast::Sender<ChatMessageResponse>) {
        let response = reqwest::Client::new()
            .post("http://localhost:11434/api/chat")
            .json::<UserChatMessageRequest>(&prompt.into())
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
                let line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<ModelResponse>(&line) {
                    Ok(event) => {
                        done = event.done;

                        if let Some(tool_calls) = event.message.tool_calls {
                            resp_tx
                                .send(ChatMessageResponse {
                                    role: event.message.role.clone(),
                                    tool_calls: Some(tool_calls.clone()),
                                    done: false,
                                    ..Default::default()
                                })
                                .unwrap();
                        }
                        if let Some(content) = event.message.content {
                            let to_push = content.clone();
                            resp_tx
                                .send(ChatMessageResponse {
                                    role: event.message.role.clone(),
                                    content: Some(content),
                                    done: false,
                                    ..Default::default()
                                })
                                .unwrap();
                            complete_response.push_str(&to_push);
                        } else if let Some(thinking) = event.message.thinking {
                            resp_tx
                                .send(ChatMessageResponse {
                                    role: event.message.role.clone(),
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
}

//{"model":"gemma4:e2b","created_at":"2026-04-26T18:36:45.348105572Z","message":{"role":"assistant","content":" a"},"done":false}
//{"model":"gemma4:e2b","created_at":"2026-04-26T18:36:45.633413016Z","message":{"role":"assistant","content":" much"},"done":false}
//{"model":"gemma4:e2b","created_at":"2026-04-26T18:36:45.90663008Z","message":{"role":"assistant","content":" thicker"},"done":false}
//{"model":"gemma4:e2b","created_at":"2026-04-26T18:36:46.185375654Z","message":{"role":"assistant","content":" layer"},"done":false}
//{"model":"gemma4:e2b","created_at":"2026-04-26T18:36:46.458949222Z","message":{"role":"assistant","content":" of"},"done":false}

#[derive(serde::Deserialize, Debug)]
struct ModelResponse {
    model: String,
    created_at: String,
    done: bool,
    message: ModelMessage,
}

#[derive(serde::Deserialize, Debug)]
struct ModelMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCallRequest>>,
}

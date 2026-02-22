use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::agent::AgentContext;
use super::mcp::McpManager;
use super::tools::{Tool, ToolCall, ToolExecutor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub model: String,
    #[serde(default, alias = "api-key")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default = "default_temperature")]
    pub temp: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub mcp_servers: HashMap<String, super::mcp::McpServerConfig>,
    #[serde(default)]
    pub workspace: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub mcp: bool,
    #[serde(default = "default_true")]
    pub tools: bool,
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> u32 {
    2048
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default, alias = "api-key")]
    api_key: Option<String>,
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    temp: Option<f32>,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    mcp_servers: HashMap<String, super::mcp::McpServerConfig>,
    #[serde(default)]
    workspace: Option<PathBuf>,
    #[serde(default)]
    mcp: Option<bool>,
    #[serde(default)]
    tools: Option<bool>,
    #[serde(default)]
    presets: HashMap<String, ConfigFile>,
}

impl Config {
    pub fn load() -> Self {
        Self::load_with_preset(None)
    }

    pub fn load_with_preset(preset: Option<&str>) -> Self {
        let preset: Option<String> = preset
            .map(|s| s.to_string())
            .or_else(|| std::env::var("RMUX_PRESET").ok());

        let config_paths: Vec<&str> = vec!["config.yml", "config.yaml"];

        for path in &config_paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(file) = serde_yaml::from_str::<ConfigFile>(&content) {
                    let config = Self::merge_config(&file, preset.as_deref());
                    if let Some(p) = &preset {
                        eprintln!("[rmux] Loaded preset '{}' from: {}", p, path);
                    } else {
                        eprintln!("[rmux] Loaded config from: {}", path);
                    }
                    if let Some(ref ws) = config.workspace {
                        eprintln!("[rmux] Workspace: {:?}", ws);
                    }
                    eprintln!("[rmux] MCP: {}, Tools: {}", config.mcp, config.tools);
                    return config;
                }
            }
        }

        if let Some(config_dir) = dirs::config_dir() {
            let path = config_dir.join("rmux/config.yml");
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(file) = serde_yaml::from_str::<ConfigFile>(&content) {
                    let config = Self::merge_config(&file, preset.as_deref());
                    eprintln!("[rmux] Loaded config from: {:?}", path);
                    return config;
                }
            }
        }

        eprintln!("[rmux] No config.yml found, using defaults");
        Self::default()
    }

    fn merge_config(file: &ConfigFile, preset: Option<&str>) -> Self {
        let mut base = file.clone();

        if let Some(preset_name) = preset {
            if let Some(preset_config) = file.presets.get(preset_name) {
                base.mode = preset_config.mode.clone().or(base.mode);
                base.endpoint = preset_config.endpoint.clone().or(base.endpoint);
                base.model = preset_config.model.clone().or(base.model);
                base.api_key = preset_config.api_key.clone().or(base.api_key);
                base.system = preset_config.system.clone().or(base.system);
                base.temp = preset_config.temp.or(base.temp);
                base.max_tokens = preset_config.max_tokens.or(base.max_tokens);
                base.workspace = preset_config.workspace.clone().or(base.workspace);
                base.mcp = preset_config.mcp.or(base.mcp);
                base.tools = preset_config.tools.or(base.tools);
                if !preset_config.mcp_servers.is_empty() {
                    base.mcp_servers = preset_config.mcp_servers.clone();
                }
            } else {
                eprintln!("[rmux] Preset '{}' not found", preset_name);
            }
        }

        Self {
            endpoint: base
                .endpoint
                .unwrap_or_else(|| "http://localhost:8080/v1".to_string()),
            model: base.model.unwrap_or_else(|| "default".to_string()),
            api_key: base.api_key,
            system: base.system,
            temp: base.temp.unwrap_or(0.7),
            max_tokens: base.max_tokens.unwrap_or(2048),
            mcp_servers: base.mcp_servers,
            workspace: base.workspace,
            mcp: base.mcp.unwrap_or(true),
            tools: base.tools.unwrap_or(true),
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Tool>>,
}

#[derive(Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Deserialize)]
struct DeltaToolCall {
    id: Option<String>,
    #[serde(rename = "type")]
    tool_type: Option<String>,
    function: Option<DeltaFunction>,
}

#[derive(Deserialize)]
struct DeltaFunction {
    name: Option<String>,
    arguments: Option<String>,
}

pub enum StreamEvent {
    Text(String),
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallDelta(String),
    ToolCallEnd,
    ToolExecuting {
        name: String,
        arguments: String,
    },
    ToolResult {
        id: String,
        name: String,
        result: String,
    },
    Done,
    Error(String),
}

fn get_history_path(workspace: &Path) -> PathBuf {
    workspace.join(".agent").join("HISTORY.log")
}

fn load_history(workspace: &Path) -> Vec<ChatMessage> {
    let history_path = get_history_path(workspace);
    if !history_path.exists() {
        return Vec::new();
    }

    if let Ok(content) = std::fs::read_to_string(&history_path) {
        let mut messages = Vec::new();
        let parts: Vec<&str> = content.split("--- END ---").collect();

        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some(content) = part.strip_prefix("--- USER ---") {
                let content = content.trim();
                if !content.is_empty() {
                    messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: Some(content.to_string()),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                }
            } else if let Some(content) = part.strip_prefix("--- ASSISTANT ---") {
                let content = content.trim();
                if !content.is_empty() {
                    messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: Some(content.to_string()),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                }
            }
        }

        if !messages.is_empty() {
            eprintln!("[rmux] Loaded {} messages from history", messages.len());
        }
        messages
    } else {
        Vec::new()
    }
}

fn save_history(workspace: &Path, messages: &[ChatMessage]) {
    let history_path = get_history_path(workspace);

    if let Some(parent) = history_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut content = String::new();
    for msg in messages {
        match msg.role.as_str() {
            "user" => {
                content.push_str(&format!(
                    "--- USER ---\n{}\n--- END ---\n\n",
                    msg.content.as_deref().unwrap_or("")
                ));
            }
            "assistant" => {
                content.push_str(&format!(
                    "--- ASSISTANT ---\n{}\n--- END ---\n\n",
                    msg.content.as_deref().unwrap_or("")
                ));
            }
            _ => {}
        }
    }

    let _ = std::fs::write(&history_path, content);
}

pub struct LlmClient {
    client: Client,
    config: Config,
    conversations: Arc<RwLock<Vec<ChatMessage>>>,
    tools: Arc<RwLock<Vec<Tool>>>,
    tool_executor: Option<ToolExecutor>,
    mcp_manager: Arc<RwLock<McpManager>>,
    agent_context: Option<AgentContext>,
    cached_headers: reqwest::header::HeaderMap,
}

impl LlmClient {
    pub fn new(config: Config) -> Self {
        let tool_executor = if config.tools && config.workspace.is_some() {
            config
                .workspace
                .as_ref()
                .map(|ws| ToolExecutor::new(ws.clone()))
        } else {
            None
        };

        let agent_context = config.workspace.as_ref().map(|ws| AgentContext::load(ws));

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        if let Some(ref api_key) = config.api_key {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key).parse().unwrap(),
            );
        }

        Self {
            client: Client::new(),
            config,
            conversations: Arc::new(RwLock::new(Vec::new())),
            tools: Arc::new(RwLock::new(Vec::new())),
            tool_executor,
            mcp_manager: Arc::new(RwLock::new(McpManager::new())),
            agent_context,
            cached_headers: headers,
        }
    }

    pub fn default_client() -> Self {
        Self::new(Config::load())
    }

    pub fn with_preset(preset: &str) -> Self {
        Self::new(Config::load_with_preset(Some(preset)))
    }

    pub fn get_workspace(&self) -> Option<&PathBuf> {
        self.config.workspace.as_ref()
    }

    fn build_system_prompt(&self) -> String {
        let mut prompt = self
            .config
            .system
            .clone()
            .unwrap_or_else(|| "You are a helpful assistant.".to_string());

        if let Some(ref agent) = self.agent_context {
            if agent.has_content() {
                prompt.push_str(&agent.to_system_prompt());
            }
        }

        if self.tool_executor.is_some() {
            prompt.push_str("\n\nYou have access to file system tools in the workspace directory. You can read, write, create, and edit files.");
        }

        prompt
    }

    pub async fn initialize(&self) -> Result<()> {
        let mut tools = self.tools.write().await;

        if self.config.tools && self.tool_executor.is_some() {
            tools.extend(ToolExecutor::get_tools());
            eprintln!("[rmux] File tools enabled (sandboxed to workspace)");
        }

        if self.config.mcp && !self.config.mcp_servers.is_empty() {
            let mcp_config = super::mcp::McpConfig {
                mcp_servers: self.config.mcp_servers.clone(),
            };
            let mut manager = self.mcp_manager.write().await;
            manager.load_from_config(&mcp_config).await?;
            tools.extend(manager.get_all_tools());
        }
        drop(tools);

        let mut messages = self.conversations.write().await;
        if messages.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: Some(self.build_system_prompt()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });

            if let Some(ref workspace) = self.config.workspace {
                let history = load_history(workspace);
                messages.extend(history);
            }
        }

        Ok(())
    }

    pub fn get_tools(&self) -> Arc<RwLock<Vec<Tool>>> {
        self.tools.clone()
    }

    pub async fn clear_conversation(&self) {
        let mut messages = self.conversations.write().await;
        messages.clear();
        // Re-add system message
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: Some(self.build_system_prompt()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    pub fn get_conversation(&self) -> Option<Vec<ChatMessage>> {
        self.conversations.try_read().ok().map(|g| g.clone())
    }

    pub async fn save_conversation(&self) {
        if let Some(ref workspace) = self.config.workspace {
            let messages = self.conversations.read().await;
            save_history(workspace, &messages);
        }
    }

    pub fn get_mcp_status(&self) -> Option<Vec<(String, bool)>> {
        self.mcp_manager.try_read().ok().map(|mcp| mcp.connection_status())
    }

    pub async fn disconnect_mcp(&self, server_name: &str) -> bool {
        let mut mcp = self.mcp_manager.write().await;
        mcp.disconnect_server(server_name)
    }

    fn process_stream_data<F>(
        data: &str,
        full_response: &mut String,
        tool_calls: &mut Vec<ToolCall>,
        on_event: &mut F,
    ) where
        F: FnMut(StreamEvent) + Send,
    {
        if let Ok(stream_response) = serde_json::from_str::<StreamResponse>(data) {
            if let Some(choice) = stream_response.choices.first() {
                if let Some(content) = &choice.delta.content {
                    full_response.push_str(content);
                    on_event(StreamEvent::Text(content.clone()));
                }

                if let Some(delta_tool_calls) = &choice.delta.tool_calls {
                    for delta_tc in delta_tool_calls {
                        if let Some(id) = &delta_tc.id {
                            let name = delta_tc
                                .function
                                .as_ref()
                                .and_then(|f| f.name.clone())
                                .unwrap_or_default();
                            let tool_type = delta_tc
                                .tool_type
                                .clone()
                                .unwrap_or_else(|| "function".to_string());
                            on_event(StreamEvent::ToolCallStart {
                                id: id.clone(),
                                name: name.clone(),
                            });
                            tool_calls.push(ToolCall {
                                id: id.clone(),
                                tool_type,
                                function: super::tools::FunctionCall {
                                    name,
                                    arguments: String::new(),
                                },
                            });
                        }

                        if let Some(args_delta) = delta_tc
                            .function
                            .as_ref()
                            .and_then(|f| f.arguments.as_ref())
                        {
                            on_event(StreamEvent::ToolCallDelta(args_delta.clone()));
                            if let Some(last_tc) = tool_calls.last_mut() {
                                last_tc.function.arguments.push_str(args_delta);
                            }
                        }
                    }
                }

                if choice.finish_reason.is_some() && !tool_calls.is_empty() {
                    on_event(StreamEvent::ToolCallEnd);
                }
            }
        } else if !data.is_empty() {
            on_event(StreamEvent::Error(format!(
                "Failed to parse stream: {}",
                data
            )));
        }
    }

    fn build_headers(&self) -> &reqwest::header::HeaderMap {
        &self.cached_headers
    }

    pub async fn send_message_stream<F>(
        &self,
        user_message: &str,
        mut on_event: F,
    ) -> Result<String>
    where
        F: FnMut(StreamEvent) + Send,
    {
        // Push user message with a brief lock
        {
            let mut messages = self.conversations.write().await;
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: Some(user_message.to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        let mut full_response = String::new();
        let tools_guard = self.tools.read().await;
        let tools_option = if tools_guard.is_empty() {
            None
        } else {
            Some(tools_guard.clone())
        };
        drop(tools_guard);

        loop {
            let messages_snapshot = {
                let messages = self.conversations.read().await;
                messages.clone()
            };

            let request = ChatRequest {
                model: self.config.model.clone(),
                messages: messages_snapshot,
                temperature: self.config.temp,
                max_tokens: self.config.max_tokens,
                stream: true,
                tools: tools_option.clone(),
            };

            let url = format!("{}/chat/completions", self.config.endpoint);

            let response = self
                .client
                .post(&url)
                .headers(self.build_headers().clone())
                .json(&request)
                .send()
                .await
                .map_err(|e| anyhow!("Request failed: {}", e))?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow!("API error: {}", error_text));
            }

            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                let chunk_str = String::from_utf8_lossy(&chunk);
                buffer.push_str(&chunk_str);

                // Parse SSE: split by line, accumulate data lines, emit on blank
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim_end().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            break;
                        }

                        Self::process_stream_data(
                            data,
                            &mut full_response,
                            &mut tool_calls,
                            &mut on_event,
                        );
                    }
                }
            }

            // Acquire lock briefly to push results
            let mut messages = self.conversations.write().await;

            if tool_calls.is_empty() {
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: if full_response.is_empty() {
                        None
                    } else {
                        Some(full_response.clone())
                    },
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
                drop(messages);
                break;
            }

            messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_calls.clone()),
                tool_call_id: None,
                name: None,
            });
            drop(messages);

            for tc in &tool_calls {
                let tool_name = &tc.function.name;
                let tool_args = &tc.function.arguments;

                on_event(StreamEvent::ToolExecuting {
                    name: tool_name.clone(),
                    arguments: tool_args.clone(),
                });

                let result = if let Some(ref executor) = self.tool_executor {
                    executor.execute(tool_name, tool_args).await
                } else {
                    Err(anyhow!("Tool not available"))
                };

                let tool_result = match result {
                    Ok(r) => r,
                    Err(_) => {
                        let mcp = self.mcp_manager.read().await;
                        if mcp.is_mcp_tool(tool_name) {
                            mcp.execute_tool(tool_name, tool_args).await?
                        } else {
                            "Tool not available. Enable tools in config.".to_string()
                        }
                    }
                };

                on_event(StreamEvent::ToolResult {
                    id: tc.id.clone(),
                    name: tool_name.clone(),
                    result: tool_result.clone(),
                });

                let mut messages = self.conversations.write().await;
                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: Some(tool_result),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                    name: Some(tool_name.clone()),
                });
            }
        }

        self.save_conversation().await;

        on_event(StreamEvent::Done);
        Ok(full_response)
    }
}

impl Default for LlmClient {
    fn default() -> Self {
        Self::default_client()
    }
}

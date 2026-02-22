use anyhow::{Result, anyhow};
use rmcp::model::Tool as McpTool;
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransport;
use rmcp::{Peer, RoleClient, Service, serve_client};
use serde::Deserialize;
use std::collections::HashMap;

use super::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    #[serde(rename = "type", default)]
    pub server_type: Option<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

pub struct ClientHandler;

impl Service<RoleClient> for ClientHandler {
    async fn handle_request(
        &self,
        _request: rmcp::model::ServerRequest,
        _context: rmcp::service::RequestContext<RoleClient>,
    ) -> Result<rmcp::model::ClientResult, rmcp::ErrorData> {
        Ok(rmcp::model::ClientResult::empty(()))
    }

    async fn handle_notification(
        &self,
        _notification: rmcp::model::ServerNotification,
        _context: rmcp::service::NotificationContext<RoleClient>,
    ) -> Result<(), rmcp::ErrorData> {
        Ok(())
    }

    fn get_info(&self) -> rmcp::model::ClientInfo {
        rmcp::model::ClientInfo::default()
    }
}

pub struct McpConnection {
    pub name: String,
    pub peer: Peer<RoleClient>,
    pub tools: Vec<McpTool>,
    pub service: RunningService<RoleClient, ClientHandler>,
}

impl McpConnection {
    pub async fn connect_local(
        name: String,
        command: String,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, String>>,
    ) -> Result<Self> {
        let mut cmd = tokio::process::Command::new(&command);

        if let Some(args) = args {
            cmd.args(args);
        }

        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let transport = TokioChildProcess::new(cmd)?;
        let service = ClientHandler;
        let running_service = serve_client(service, transport).await?;
        let peer = running_service.peer().clone();

        let tools_result = peer.list_tools(None).await?;
        let tools = tools_result.tools;

        Ok(Self {
            name,
            peer,
            tools,
            service: running_service,
        })
    }

    pub async fn connect_remote(name: String, url: String) -> Result<Self> {
        let transport = StreamableHttpClientTransport::from_uri(url);
        let service = ClientHandler;
        let running_service = serve_client(service, transport).await?;
        let peer = running_service.peer().clone();

        let tools_result = peer.list_tools(None).await?;
        let tools = tools_result.tools;

        Ok(Self {
            name,
            peer,
            tools,
            service: running_service,
        })
    }

    pub fn is_running(&self) -> bool {
        true
    }

    pub fn shutdown(self) {
        tokio::spawn(async move {
            let _ = self.service.cancel().await;
        });
    }

    pub fn get_tools(&self) -> Vec<Tool> {
        self.tools
            .iter()
            .map(|tool| Tool {
                tool_type: "function".to_string(),
                function: super::tools::ToolFunction {
                    name: format!("mcp_{}_{}", self.name, tool.name),
                    description: tool.description.clone().unwrap_or_default().to_string(),
                    parameters: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
                },
            })
            .collect()
    }

    pub async fn execute_tool(&self, tool_name: &str, arguments: &str) -> Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments)?;
        let arguments = args.as_object().cloned();

        use rmcp::model::CallToolRequestParams;

        let params = CallToolRequestParams {
            name: tool_name.to_string().into(),
            arguments,
            meta: None,
            task: None,
        };

        let result = self
            .peer
            .call_tool(params)
            .await
            .map_err(|e| anyhow!("MCP tool execution failed: {}", e))?;

        let content_str = result
            .content
            .into_iter()
            .map(|c| {
                use rmcp::model::RawContent;
                match c.raw {
                    RawContent::Text(text) => text.text,
                    RawContent::Image(img) => format!("[Image: {}]", img.mime_type),
                    RawContent::Resource(res) => format!("[Resource: {:?}]", res.resource),
                    RawContent::Audio(audio) => format!("[Audio: {}]", audio.mime_type),
                    RawContent::ResourceLink(link) => format!("[Resource Link: {:?}]", link),
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(if content_str.is_empty() {
            "No result".to_string()
        } else {
            content_str
        })
    }
}

pub struct McpManager {
    connections: Vec<McpConnection>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
        }
    }

    pub async fn load_from_config(&mut self, config: &McpConfig) -> Result<()> {
        for (server_name, server_config) in &config.mcp_servers {
            if !server_config.enabled {
                continue;
            }

            let server_type = server_config.server_type.as_deref().unwrap_or("local");

            let connection = match server_type {
                "remote" => {
                    let url = server_config
                        .url
                        .as_ref()
                        .ok_or_else(|| anyhow!("Remote MCP server {} missing URL", server_name))?;
                    McpConnection::connect_remote(server_name.clone(), url.clone()).await
                }
                _ => {
                    let command = server_config.command.as_ref().ok_or_else(|| {
                        anyhow!("Local MCP server {} missing command", server_name)
                    })?;
                    McpConnection::connect_local(
                        server_name.clone(),
                        command.clone(),
                        server_config.args.clone(),
                        server_config.env.clone(),
                    )
                    .await
                }
            };

            match connection {
                Ok(conn) => {
                    eprintln!(
                        "[rmux] Connected to MCP server '{}' ({} tools)",
                        server_name,
                        conn.tools.len()
                    );
                    self.connections.push(conn);
                }
                Err(e) => {
                    eprintln!(
                        "[rmux] Failed to connect to MCP server '{}': {}",
                        server_name, e
                    );
                }
            }
        }

        Ok(())
    }

    pub fn get_all_tools(&self) -> Vec<Tool> {
        self.connections
            .iter()
            .flat_map(|c| c.get_tools())
            .collect()
    }

    pub async fn execute_tool(&self, full_name: &str, arguments: &str) -> Result<String> {
        let parts: Vec<&str> = full_name.splitn(3, '_').collect();
        if parts.len() < 3 {
            return Err(anyhow!("Invalid MCP tool name format: {}", full_name));
        }

        let server_name = parts[1];
        let tool_name = parts[2];

        let connection = self
            .connections
            .iter()
            .find(|c| c.name == server_name)
            .ok_or_else(|| anyhow!("MCP server '{}' not found", server_name))?;

        connection.execute_tool(tool_name, arguments).await
    }

    pub fn is_mcp_tool(&self, tool_name: &str) -> bool {
        tool_name.starts_with("mcp_")
    }

    pub fn connection_status(&self) -> Vec<(String, bool)> {
        self.connections
            .iter()
            .map(|c| (c.name.clone(), c.is_running()))
            .collect()
    }

    pub fn disconnect_server(&mut self, server_name: &str) -> bool {
        let idx = self.connections.iter().position(|c| c.name == server_name);
        if let Some(i) = idx {
            let conn = self.connections.remove(i);
            conn.shutdown();
            true
        } else {
            false
        }
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

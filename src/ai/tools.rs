use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

pub struct ToolExecutor {
    workspace: PathBuf,
}

impl ToolExecutor {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    pub fn get_tools() -> Vec<Tool> {
        vec![
            Tool {
                tool_type: "function".to_string(),
                function: ToolFunction {
                    name: "read_file".to_string(),
                    description: "Read the contents of a file from the workspace".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Relative path to the file within the workspace"
                            }
                        },
                        "required": ["path"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: ToolFunction {
                    name: "write_file".to_string(),
                    description: "Write content to a file in the workspace. Creates the file if it doesn't exist, overwrites if it does.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Relative path to the file within the workspace" },
                            "content": { "type": "string", "description": "Content to write to the file" }
                        },
                        "required": ["path", "content"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: ToolFunction {
                    name: "list_directory".to_string(),
                    description: "List the contents of a directory in the workspace".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Relative path to the directory (use '.' for workspace root)" }
                        },
                        "required": ["path"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: ToolFunction {
                    name: "create_directory".to_string(),
                    description: "Create a new directory in the workspace. Creates parent directories if needed.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Relative path to the directory within the workspace" }
                        },
                        "required": ["path"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: ToolFunction {
                    name: "edit_file".to_string(),
                    description: "Edit a file by replacing a specific string with another string.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Relative path to the file" },
                            "old_text": { "type": "string", "description": "The exact text to find and replace" },
                            "new_text": { "type": "string", "description": "The text to replace with" }
                        },
                        "required": ["path", "old_text", "new_text"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: ToolFunction {
                    name: "execute".to_string(),
                    description: "Execute a shell command in the workspace directory.".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "command": {
                                "type": "string",
                                "description": "Shell command to execute in the workspace"
                            }
                        },
                        "required": ["command"]
                    }),
                },
            },
        ]
    }

    fn validate_path(&self, relative_path: &str) -> Result<PathBuf> {
        let canonical_workspace = self
            .workspace
            .canonicalize()
            .map_err(|e| anyhow!("Workspace directory does not exist: {}", e))?;

        let full_path = self.workspace.join(relative_path);

        if full_path.exists() {
            let canonical_path = full_path
                .canonicalize()
                .map_err(|e| anyhow!("Failed to resolve path: {}", e))?;

            if !canonical_path.starts_with(&canonical_workspace) {
                bail!("Path is outside workspace");
            }
        } else {
            let mut current = full_path.parent();
            let mut found_valid_parent = false;

            while let Some(parent) = current {
                if parent.exists() {
                    let canonical_parent = parent
                        .canonicalize()
                        .map_err(|e| anyhow!("Failed to resolve parent path: {}", e))?;

                    if !canonical_parent.starts_with(&canonical_workspace) {
                        bail!("Path is outside workspace");
                    }
                    found_valid_parent = true;
                    break;
                }
                current = parent.parent();
            }

            if !found_valid_parent {
                let path_str = full_path.to_string_lossy();
                let workspace_str = canonical_workspace.to_string_lossy();

                if !path_str.starts_with(&*workspace_str) {
                    bail!("Path is outside workspace");
                }
            }
        }

        Ok(full_path)
    }

    pub async fn execute(&self, name: &str, arguments: &str) -> Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments)?;

        match name {
            "read_file" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing path"))?;
                let full_path = self.validate_path(path)?;

                let content = tokio::fs::read_to_string(&full_path)
                    .await
                    .map_err(|e| anyhow!("Failed to read file {}: {}", path, e))?;

                Ok(format!("File contents of {}:\n\n{}", path, content))
            }
            "write_file" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing path"))?;
                let content = args["content"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing content"))?;
                let full_path = self.validate_path(path)?;

                if let Some(parent) = full_path.parent() {
                    tokio::fs::create_dir_all(parent).await.ok();
                }

                tokio::fs::write(&full_path, content)
                    .await
                    .map_err(|e| anyhow!("Failed to write file {}: {}", path, e))?;

                Ok(format!(
                    "Successfully wrote {} bytes to {}",
                    content.len(),
                    path
                ))
            }
            "list_directory" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing path"))?;
                let full_path = self.validate_path(path)?;

                let mut entries = tokio::fs::read_dir(&full_path)
                    .await
                    .map_err(|e| anyhow!("Failed to read directory {}: {}", path, e))?;

                let mut result = format!("Contents of {}:\n", path);
                let mut files = Vec::new();
                let mut dirs = Vec::new();

                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if entry.file_type().await?.is_dir() {
                        dirs.push(format!("[DIR]  {}/", name));
                    } else {
                        files.push(format!("[FILE] {}", name));
                    }
                }

                dirs.sort();
                files.sort();

                if dirs.is_empty() && files.is_empty() {
                    result.push_str("  (empty)\n");
                } else {
                    for d in dirs {
                        result.push_str(&format!("  {}\n", d));
                    }
                    for f in files {
                        result.push_str(&format!("  {}\n", f));
                    }
                }

                Ok(result)
            }
            "create_directory" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing path"))?;
                let full_path = self.validate_path(path)?;

                tokio::fs::create_dir_all(&full_path)
                    .await
                    .map_err(|e| anyhow!("Failed to create directory {}: {}", path, e))?;

                Ok(format!("Successfully created directory {}", path))
            }
            "edit_file" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing path"))?;
                let old_text = args["old_text"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing old_text"))?;
                let new_text = args["new_text"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing new_text"))?;
                let full_path = self.validate_path(path)?;

                let content = tokio::fs::read_to_string(&full_path)
                    .await
                    .map_err(|e| anyhow!("Failed to read file {}: {}", path, e))?;

                if !content.contains(old_text) {
                    return Err(anyhow!(
                        "Could not find the text to replace in file {}",
                        path
                    ));
                }

                let occurrences = content.matches(old_text).count();
                let new_content = content.replace(old_text, new_text);

                tokio::fs::write(&full_path, &new_content)
                    .await
                    .map_err(|e| anyhow!("Failed to write file {}: {}", path, e))?;

                Ok(format!(
                    "Successfully replaced {} occurrence(s) in {}",
                    occurrences, path
                ))
            }
            "execute" => {
                let command = args["command"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing command"))?;

                let output = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .current_dir(&self.workspace)
                    .output()
                    .await
                    .map_err(|e| anyhow!("Failed to execute command: {}", e))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    if stdout.is_empty() && stderr.is_empty() {
                        Ok("Command executed successfully (no output)".to_string())
                    } else if stdout.is_empty() {
                        Ok(format!("stderr:\n{}", stderr))
                    } else {
                        Ok(stdout.to_string())
                    }
                } else {
                    Ok(format!(
                        "Command exited with code {:?}\nstdout: {}\nstderr: {}",
                        output.status.code(),
                        stdout,
                        stderr
                    ))
                }
            }
            _ => Err(anyhow!("Unknown tool: {}", name)),
        }
    }
}

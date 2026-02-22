use std::path::Path;

pub struct AgentContext {
    files: Vec<(String, String)>,
}

impl AgentContext {
    pub fn load(workspace: &Path) -> Self {
        let agent_dir = workspace.join(".agent");

        if !agent_dir.exists() {
            let _ = std::fs::create_dir_all(&agent_dir);
        }

        let mut files = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&agent_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == "md" {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    files.push((name.to_string(), content));
                                }
                            }
                        }
                    }
                }
            }
        }

        files.sort_by(|a, b| a.0.cmp(&b.0));

        if !files.is_empty() {
            eprintln!("[rmux] Loaded {} agent file(s) from .agent/", files.len());
        }

        Self { files }
    }

    pub fn to_system_prompt(&self) -> String {
        if self.files.is_empty() {
            String::new()
        } else {
            let content = self
                .files
                .iter()
                .map(|(name, content)| format!("# {}\n{}", name, content))
                .collect::<Vec<_>>()
                .join("\n\n");

            format!("\n\n# Agent Context\n\n{}", content)
        }
    }

    pub fn has_content(&self) -> bool {
        !self.files.is_empty()
    }
}

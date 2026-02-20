use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub working_directory: Option<String>,
    pub git_branch: Option<String>,
}

impl Workspace {
    pub fn new(id: Uuid, name: String) -> Self {
        Self {
            id,
            name,
            working_directory: None,
            git_branch: None,
        }
    }

    pub fn update_from_cwd(&mut self, cwd: &str) {
        self.working_directory = Some(cwd.to_string());
    }

    pub fn set_git_branch(&mut self, branch: &str) {
        self.git_branch = Some(branch.to_string());
    }
}

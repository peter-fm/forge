use crate::commands::init::{InitOptions, init_project};
use crate::detect::{DetectedProject, ProjectType};
use crate::error::ForgeError;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateOptions {
    pub project_type: Option<ProjectType>,
    pub force: bool,
}

pub fn generate_project(
    root: &Path,
    options: &GenerateOptions,
) -> Result<DetectedProject, ForgeError> {
    init_project(
        root,
        &InitOptions {
            project_type: options.project_type,
            force: options.force,
        },
    )
}

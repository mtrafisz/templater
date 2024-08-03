use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Template not found: {0}")]
    TemplateNotFound(String),
    #[error("Invalid template directory: {0}")]
    InvalidTemplateDir(std::path::PathBuf),
    #[error("Template {0} already exists")]
    TemplateExists(String),
    #[error("Failed to create template: {0}")]
    CreateTemplate(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Failed to edit template: {0}")]
    EditTemplate(String),
}

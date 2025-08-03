pub mod ftp;
pub mod smb;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Local};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RemoteFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: DateTime<Local>,
    pub is_dir: bool,
}

#[async_trait]
pub trait FileServerClient: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn list_files(&mut self, path: &str) -> Result<Vec<RemoteFile>>;
    async fn download_file(&mut self, remote_path: &str, local_path: &Path) -> Result<()>;
    async fn upload_file(&mut self, local_path: &Path, remote_path: &str) -> Result<()>;
    async fn create_directory(&mut self, path: &str) -> Result<()>;
    async fn delete_file(&mut self, path: &str) -> Result<()>;
    async fn get_file_size(&mut self, path: &str) -> Result<u64>;
}

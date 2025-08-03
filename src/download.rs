use crate::client::FileServerClient;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ParallelDownloader {
    client: Arc<Mutex<Box<dyn FileServerClient>>>,
    max_concurrent: usize,
}

impl ParallelDownloader {
    pub fn new(client: Arc<Mutex<Box<dyn FileServerClient>>>, max_concurrent: usize) -> Self {
        Self {
            client,
            max_concurrent,
        }
    }

    pub async fn download_files(
        &self,
        files: Vec<(String, PathBuf)>, // (remote_path, local_path)
    ) -> Result<Vec<Result<()>>> {
        let multi_progress = MultiProgress::new();

        let results = stream::iter(files)
            .map(|(remote_path, local_path)| {
                let client = self.client.clone();
                let pb = multi_progress.add(ProgressBar::new(0));

                async move {
                    self.download_single_file(client, remote_path, local_path, pb)
                        .await
                }
            })
            .buffer_unordered(self.max_concurrent)
            .collect::<Vec<_>>()
            .await;

        Ok(results)
    }

    async fn download_single_file(
        &self,
        client: Arc<Mutex<Box<dyn FileServerClient>>>,
        remote_path: String,
        local_path: PathBuf,
        progress_bar: ProgressBar,
    ) -> Result<()> {
        // Set up progress bar style
        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")?
                .progress_chars("#>-"),
        );

        let filename = remote_path.split('/').last().unwrap_or("file");
        progress_bar.set_message(format!("Downloading {}", filename));

        // Get file size first
        let mut client_guard = client.lock().await;
        let file_size = client_guard.get_file_size(&remote_path).await?;
        drop(client_guard);

        progress_bar.set_length(file_size);

        // Create parent directory if needed
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Download the file
        let mut client_guard = client.lock().await;
        client_guard
            .download_file(&remote_path, &local_path)
            .await?;

        progress_bar.finish_with_message(format!("✓ {}", filename));
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn download_directory(
        &self,
        remote_dir: &str,
        local_dir: &Path,
    ) -> Result<Vec<Result<()>>> {
        // List all files in the directory
        let mut client_guard = self.client.lock().await;
        let files = client_guard.list_files(remote_dir).await?;
        drop(client_guard);

        // Filter out directories and prepare download list
        let download_list: Vec<(String, PathBuf)> = files
            .into_iter()
            .filter(|f| !f.is_dir)
            .map(|f| {
                let local_path = local_dir.join(&f.name);
                (f.path, local_path)
            })
            .collect();

        self.download_files(download_list).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{FileServerClient, RemoteFile};
    use async_trait::async_trait;
    use chrono::Local;
    use mockall::mock;

    mock! {
        TestClient {}

        #[async_trait]
        impl FileServerClient for TestClient {
            async fn connect(&mut self) -> Result<()>;
            async fn disconnect(&mut self) -> Result<()>;
            async fn list_files(&mut self, path: &str) -> Result<Vec<RemoteFile>>;
            async fn download_file(&mut self, remote_path: &str, local_path: &Path) -> Result<()>;
            async fn upload_file(&mut self, local_path: &Path, remote_path: &str) -> Result<()>;
            async fn create_directory(&mut self, path: &str) -> Result<()>;
            async fn delete_file(&mut self, path: &str) -> Result<()>;
            async fn get_file_size(&mut self, path: &str) -> Result<u64>;
        }
    }

    #[tokio::test]
    async fn test_parallel_downloader_creation() {
        let mut mock_client = MockTestClient::new();
        mock_client.expect_connect().returning(|| Ok(()));

        let client: Box<dyn FileServerClient> = Box::new(mock_client);
        let client = Arc::new(Mutex::new(client));

        let downloader = ParallelDownloader::new(client, 4);
        assert_eq!(downloader.max_concurrent, 4);
    }

    #[tokio::test]
    async fn test_download_directory_filters_directories() {
        let mut mock_client = MockTestClient::new();

        // Mock list_files to return mix of files and directories
        mock_client
            .expect_list_files()
            .with(mockall::predicate::eq("/test"))
            .returning(|_| {
                Ok(vec![
                    RemoteFile {
                        name: "file1.txt".to_string(),
                        path: "/test/file1.txt".to_string(),
                        size: 100,
                        modified: Local::now(),
                        is_dir: false,
                    },
                    RemoteFile {
                        name: "subdir".to_string(),
                        path: "/test/subdir".to_string(),
                        size: 0,
                        modified: Local::now(),
                        is_dir: true,
                    },
                    RemoteFile {
                        name: "file2.pdf".to_string(),
                        path: "/test/file2.pdf".to_string(),
                        size: 200,
                        modified: Local::now(),
                        is_dir: false,
                    },
                ])
            });

        // Expect get_file_size calls only for files, not directories
        mock_client
            .expect_get_file_size()
            .times(2)
            .returning(|path| {
                if path.ends_with("file1.txt") {
                    Ok(100)
                } else {
                    Ok(200)
                }
            });

        // Expect download_file calls only for files
        mock_client
            .expect_download_file()
            .times(2)
            .returning(|_, _| Ok(()));

        let client: Box<dyn FileServerClient> = Box::new(mock_client);
        let client = Arc::new(Mutex::new(client));

        let downloader = ParallelDownloader::new(client, 2);
        let temp_dir = tempfile::tempdir().unwrap();

        let results = downloader
            .download_directory("/test", temp_dir.path())
            .await
            .unwrap();
        assert_eq!(results.len(), 2); // Only 2 files, not the directory
    }
}

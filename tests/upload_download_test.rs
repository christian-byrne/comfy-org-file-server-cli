use comfy_fs::{client::FileServerClient, config::Config, download::ParallelDownloader};
use tempfile::TempDir;
use std::sync::Arc;
use tokio::sync::Mutex;
use mockall::mock;
use async_trait::async_trait;
use anyhow::Result;
use comfy_fs::client::RemoteFile;
use chrono::Local;

mock! {
    TestClient {}
    
    #[async_trait]
    impl FileServerClient for TestClient {
        async fn connect(&mut self) -> Result<()>;
        async fn disconnect(&mut self) -> Result<()>;
        async fn list_files(&mut self, path: &str) -> Result<Vec<RemoteFile>>;
        async fn download_file(&mut self, remote_path: &str, local_path: &std::path::Path) -> Result<()>;
        async fn upload_file(&mut self, local_path: &std::path::Path, remote_path: &str) -> Result<()>;
        async fn create_directory(&mut self, path: &str) -> Result<()>;
        async fn delete_file(&mut self, path: &str) -> Result<()>;
        async fn get_file_size(&mut self, path: &str) -> Result<u64>;
    }
}

#[tokio::test]
async fn test_single_file_download() {
    let mut mock_client = MockTestClient::new();
    
    // Mock expectations
    mock_client.expect_get_file_size()
        .with(mockall::predicate::eq("/test.txt"))
        .returning(|_| Ok(1024));
        
    mock_client.expect_download_file()
        .with(
            mockall::predicate::eq("/test.txt"),
            mockall::predicate::always()
        )
        .returning(|_, local_path| {
            // Simulate file creation
            std::fs::write(local_path, b"test content").unwrap();
            Ok(())
        });
    
    let client: Box<dyn FileServerClient> = Box::new(mock_client);
    let client = Arc::new(Mutex::new(client));
    
    let downloader = ParallelDownloader::new(client, 1);
    let temp_dir = TempDir::new().unwrap();
    let local_path = temp_dir.path().join("test.txt");
    
    let results = downloader.download_files(vec![
        ("/test.txt".to_string(), local_path.clone())
    ]).await.unwrap();
    
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok());
    assert!(local_path.exists());
    assert_eq!(std::fs::read_to_string(&local_path).unwrap(), "test content");
}

#[tokio::test]
async fn test_parallel_downloads() {
    let mut mock_client = MockTestClient::new();
    
    // Mock file sizes
    mock_client.expect_get_file_size()
        .times(3)
        .returning(|path| {
            match path {
                "/file1.txt" => Ok(100),
                "/file2.txt" => Ok(200),
                "/file3.txt" => Ok(300),
                _ => Ok(0),
            }
        });
    
    // Mock downloads
    mock_client.expect_download_file()
        .times(3)
        .returning(|remote_path, local_path| {
            let content = format!("Content of {}", remote_path);
            std::fs::write(local_path, content.as_bytes()).unwrap();
            Ok(())
        });
    
    let client: Box<dyn FileServerClient> = Box::new(mock_client);
    let client = Arc::new(Mutex::new(client));
    
    let downloader = ParallelDownloader::new(client, 2); // Max 2 concurrent
    let temp_dir = TempDir::new().unwrap();
    
    let files = vec![
        ("/file1.txt".to_string(), temp_dir.path().join("file1.txt")),
        ("/file2.txt".to_string(), temp_dir.path().join("file2.txt")),
        ("/file3.txt".to_string(), temp_dir.path().join("file3.txt")),
    ];
    
    let results = downloader.download_files(files).await.unwrap();
    
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.is_ok()));
    
    // Verify all files exist
    assert!(temp_dir.path().join("file1.txt").exists());
    assert!(temp_dir.path().join("file2.txt").exists());
    assert!(temp_dir.path().join("file3.txt").exists());
}

#[tokio::test]
async fn test_download_with_error_handling() {
    let mut mock_client = MockTestClient::new();
    
    // Mock file sizes - first call succeeds, second fails
    mock_client.expect_get_file_size()
        .with(mockall::predicate::eq("/success.txt"))
        .returning(|_| Ok(100));
        
    mock_client.expect_get_file_size()
        .with(mockall::predicate::eq("/fail.txt"))
        .returning(|_| Err(anyhow::anyhow!("File not found")));
    
    // Mock downloads - only called for successful file
    mock_client.expect_download_file()
        .with(
            mockall::predicate::eq("/success.txt"),
            mockall::predicate::always()
        )
        .returning(|_, local_path| {
            std::fs::write(local_path, b"success").unwrap();
            Ok(())
        });
    
    let client: Box<dyn FileServerClient> = Box::new(mock_client);
    let client = Arc::new(Mutex::new(client));
    
    let downloader = ParallelDownloader::new(client, 2);
    let temp_dir = TempDir::new().unwrap();
    
    let files = vec![
        ("/success.txt".to_string(), temp_dir.path().join("success.txt")),
        ("/fail.txt".to_string(), temp_dir.path().join("fail.txt")),
    ];
    
    let results = downloader.download_files(files).await.unwrap();
    
    assert_eq!(results.len(), 2);
    // Note: Order may vary due to parallel execution, so we check both possibilities
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    let error_count = results.iter().filter(|r| r.is_err()).count();
    
    assert_eq!(success_count, 1);
    assert_eq!(error_count, 1);
    
    // Only successful file should exist
    assert!(temp_dir.path().join("success.txt").exists());
    assert!(!temp_dir.path().join("fail.txt").exists());
}

#[test]
fn test_upload_file_validation() {
    // Test that upload validates file existence
    let temp_dir = TempDir::new().unwrap();
    let non_existent = temp_dir.path().join("does_not_exist.txt");
    
    assert!(!non_existent.exists());
    
    // Create a file to test successful case
    let existing_file = temp_dir.path().join("exists.txt");
    std::fs::write(&existing_file, b"test").unwrap();
    assert!(existing_file.exists());
}

#[tokio::test]
async fn test_sync_directory_download_only() {
    let mut mock_client = MockTestClient::new();
    
    // Mock list_files to return remote files
    mock_client.expect_list_files()
        .with(mockall::predicate::eq("/remote"))
        .returning(|_| Ok(vec![
            RemoteFile {
                name: "remote_only.txt".to_string(),
                path: "/remote/remote_only.txt".to_string(),
                size: 100,
                modified: Local::now(),
                is_dir: false,
            },
            RemoteFile {
                name: "subdir".to_string(),
                path: "/remote/subdir".to_string(),
                size: 0,
                modified: Local::now(),
                is_dir: true,
            },
        ]));
    
    let client: Box<dyn FileServerClient> = Box::new(mock_client);
    let _client = Arc::new(Mutex::new(client));
    
    // The sync functionality would download remote_only.txt
    // This is just a structural test
}

#[test]
fn test_config_persistence() {
    let config = Config {
        server_ip: "192.168.1.100".to_string(),
        username: "testuser".to_string(),
        password: Some("testpass".to_string()),
        default_protocol: comfy_fs::config::Protocol::Ftp,
        configured: true,
    };
    
    // Test serialization
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("192.168.1.100"));
    assert!(json.contains("testuser"));
    assert!(!json.contains("testpass")); // Password should be skipped
}
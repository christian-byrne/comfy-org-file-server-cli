use assert_fs::prelude::*;
use assert_fs::TempDir;
use fake::{Fake, Faker};
use rstest::*;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

/// Test configuration fixture
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub server_ip: String,
    pub username: String,
    pub password: String,
    pub temp_dir: PathBuf,
}

/// Creates a test configuration with fake data
#[fixture]
pub fn test_config() -> TestConfig {
    let temp_dir = TempDir::new().unwrap();
    TestConfig {
        server_ip: "192.168.8.156".to_string(),
        username: "test_user".to_string(),
        password: "test_password".to_string(),
        temp_dir: temp_dir.path().to_path_buf(),
    }
}

/// Mock file entries for testing
#[derive(Debug, Clone)]
pub struct MockFileEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: chrono::DateTime<chrono::Local>,
}

/// Generates random file entries
#[fixture]
pub fn mock_files() -> Vec<MockFileEntry> {
    (0..10)
        .map(|i| {
            let is_dir = i % 3 == 0;
            MockFileEntry {
                name: if is_dir {
                    format!("dir_{}", Faker.fake::<String>())
                } else {
                    format!("file_{}.{}", Faker.fake::<String>(), ["txt", "pdf", "doc", "png"][i % 4])
                },
                size: if is_dir { 0 } else { (100..10_000_000).fake() },
                is_dir,
                modified: chrono::Local::now() - chrono::Duration::hours((0..720).fake()),
            }
        })
        .collect()
}

/// Creates a mock FTP server
#[fixture]
pub async fn mock_ftp_server() -> MockServer {
    MockServer::start().await
}

/// Creates a mock SMB endpoint
#[fixture]
pub async fn mock_smb_server() -> MockServer {
    MockServer::start().await
}

/// Test file system setup
#[fixture]
pub fn test_filesystem() -> TestFilesystem {
    let temp_dir = TempDir::new().unwrap();
    
    // Create test directory structure
    temp_dir.child("Documents").create_dir_all().unwrap();
    temp_dir.child("Downloads").create_dir_all().unwrap();
    temp_dir.child("Projects").create_dir_all().unwrap();
    
    // Create test files
    temp_dir
        .child("test.txt")
        .write_str("Hello, test!")
        .unwrap();
    temp_dir
        .child("Documents/report.pdf")
        .write_str("PDF content")
        .unwrap();
    temp_dir
        .child("Projects/code.rs")
        .write_str("fn main() {}")
        .unwrap();
    
    TestFilesystem {
        root: temp_dir,
    }
}

pub struct TestFilesystem {
    pub root: TempDir,
}

impl TestFilesystem {
    pub fn path(&self) -> &std::path::Path {
        self.root.path()
    }
}

/// Assertion helpers
pub mod assertions {
    use pretty_assertions::assert_eq;
    
    pub fn assert_sorted_by_modified(files: &[super::MockFileEntry], reverse: bool) {
        let mut sorted = files.to_vec();
        sorted.sort_by(|a, b| {
            if reverse {
                a.modified.cmp(&b.modified)
            } else {
                b.modified.cmp(&a.modified)
            }
        });
        assert_eq!(files, &sorted);
    }
    
    pub fn assert_sorted_by_name(files: &[super::MockFileEntry], reverse: bool) {
        let mut sorted = files.to_vec();
        sorted.sort_by(|a, b| {
            let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
            if reverse {
                cmp.reverse()
            } else {
                cmp
            }
        });
        assert_eq!(files, &sorted);
    }
}

/// Mock server response builders
pub mod mock_responses {
    use super::*;
    
    pub fn ftp_list_response(files: &[MockFileEntry]) -> String {
        files
            .iter()
            .map(|f| {
                if f.is_dir {
                    format!("drwxr-xr-x 2 user group {} {} {}", 
                        f.size, 
                        f.modified.format("%b %d %H:%M"), 
                        f.name)
                } else {
                    format!("-rw-r--r-- 1 user group {} {} {}", 
                        f.size, 
                        f.modified.format("%b %d %H:%M"), 
                        f.name)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    
    pub fn setup_ftp_mocks(server: &MockServer, files: &[MockFileEntry]) {
        Mock::given(method("LIST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ftp_list_response(files)))
            .mount(server)
            .await;
    }
}
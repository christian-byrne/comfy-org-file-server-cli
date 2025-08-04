use super::{FileServerClient, RemoteFile};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Local;
use std::path::Path;
use std::process::Command;
use tokio::process::Command as TokioCommand;

pub struct SmbClient {
    host: String,
    username: String,
    password: String,
    share: String,
}

impl SmbClient {
    pub fn new(host: String, username: String, password: String, share: Option<String>) -> Self {
        Self {
            host,
            username,
            password,
            share: share.unwrap_or_else(|| "share".to_string()),
        }
    }


    async fn run_smbclient_command(&self, args: &[&str]) -> Result<String> {
        let mut cmd = TokioCommand::new("smbclient");
        cmd.args(args);
        cmd.arg("-U").arg(format!("{}%{}", self.username, self.password));
        cmd.arg("-N"); // No password prompt
        
        
        let output = cmd.output().await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("SMB command failed: {}", stderr));
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn parse_smbclient_list(&self, output: &str, base_path: &str) -> Vec<RemoteFile> {
        let mut files = Vec::new();
        
        for line in output.lines() {
            if let Some(file) = self.parse_list_line(line, base_path) {
                files.push(file);
            }
        }
        
        files
    }

    fn parse_list_line(&self, line: &str, base_path: &str) -> Option<RemoteFile> {
        // Parse smbclient ls output format:
        //   filename                          D        0  Wed Dec 25 10:30:45 2024
        //   filename                         AH     1234  Wed Dec 25 10:30:45 2024
        
        // Skip empty lines and the disk space summary line
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.contains("blocks of size") {
            return None;
        }
        
        // SMB output has fixed-width columns, need to parse more carefully
        // First 35 chars are filename (padded), then attributes, size, date
        
        if line.len() < 36 {
            return None;
        }
        
        // Extract filename (first 35 chars, trimmed)
        let name = line[..35].trim();
        
        // Skip current and parent directory entries
        if name == "." || name == ".." {
            return None;
        }
        
        // Rest of the line contains attributes, size, and date
        let rest = line[35..].trim();
        let parts: Vec<&str> = rest.split_whitespace().collect();
        
        if parts.is_empty() {
            return None;
        }
        
        let attributes = parts[0];
        let is_dir = attributes.contains('D');
        
        let size = if parts.len() > 1 && !is_dir {
            parts[1].parse::<u64>().unwrap_or(0)
        } else {
            0
        };
        
        // Parse date - simplified approach
        let modified = Local::now(); // TODO: Parse actual date from SMB output
        
        let path = if base_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", base_path.trim_end_matches('/'), name)
        };
        
        Some(RemoteFile {
            name: name.to_string(),
            path,
            size,
            modified,
            is_dir,
        })
    }

    fn check_smbclient_available() -> Result<()> {
        let output = Command::new("smbclient")
            .arg("--version")
            .output();
            
        match output {
            Ok(output) if output.status.success() => Ok(()),
            _ => Err(anyhow!("smbclient not found. Please install samba-client package")),
        }
    }
}

#[async_trait]
impl FileServerClient for SmbClient {
    async fn connect(&mut self) -> Result<()> {
        // Check if smbclient is available
        Self::check_smbclient_available()?;
        
        // Test connection by listing root directory
        let smb_path = format!("//{}/{}", self.host, self.share);
        let args = vec![&smb_path, "-c", "ls"];
        
        self.run_smbclient_command(&args).await?;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // Nothing to do for SMB - each command is a separate connection
        Ok(())
    }

    async fn list_files(&mut self, path: &str) -> Result<Vec<RemoteFile>> {
        let smb_path = format!("//{}/{}", self.host, self.share);
        let clean_path = path.trim_start_matches('/');
        
        let ls_command = if clean_path.is_empty() {
            "ls".to_string()
        } else {
            format!("cd {}; ls", clean_path)
        };
        
        let args = vec![&smb_path, "-c", &ls_command];
        let output = self.run_smbclient_command(&args).await?;
        
        Ok(self.parse_smbclient_list(&output, path))
    }

    async fn download_file(&mut self, remote_path: &str, local_path: &Path) -> Result<()> {
        let smb_path = format!("//{}/{}", self.host, self.share);
        let clean_remote = remote_path.trim_start_matches('/');
        
        // Create parent directory if needed
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        let local_str = local_path.to_string_lossy();
        let get_command = format!("get {} {}", clean_remote, local_str);
        let args = vec![&smb_path, "-c", &get_command];
        
        self.run_smbclient_command(&args).await?;
        Ok(())
    }

    async fn upload_file(&mut self, local_path: &Path, remote_path: &str) -> Result<()> {
        let smb_path = format!("//{}/{}", self.host, self.share);
        let clean_remote = remote_path.trim_start_matches('/');
        let local_str = local_path.to_string_lossy();
        
        let put_command = format!("put {} {}", local_str, clean_remote);
        let args = vec![&smb_path, "-c", &put_command];
        
        self.run_smbclient_command(&args).await?;
        Ok(())
    }

    async fn create_directory(&mut self, path: &str) -> Result<()> {
        let smb_path = format!("//{}/{}", self.host, self.share);
        let clean_path = path.trim_start_matches('/');
        
        let mkdir_command = format!("mkdir {}", clean_path);
        let args = vec![&smb_path, "-c", &mkdir_command];
        
        self.run_smbclient_command(&args).await?;
        Ok(())
    }

    async fn delete_file(&mut self, path: &str) -> Result<()> {
        let smb_path = format!("//{}/{}", self.host, self.share);
        let clean_path = path.trim_start_matches('/');
        
        let del_command = format!("del {}", clean_path);
        let args = vec![&smb_path, "-c", &del_command];
        
        self.run_smbclient_command(&args).await?;
        Ok(())
    }

    async fn get_file_size(&mut self, path: &str) -> Result<u64> {
        // For SMB, we'll list the parent directory and find the file
        let parent_path = if let Some(pos) = path.rfind('/') {
            &path[..pos]
        } else {
            "/"
        };
        
        let filename = path.split('/').last().unwrap_or("");
        
        let files = self.list_files(parent_path).await?;
        
        for file in files {
            if file.name == filename {
                return Ok(file.size);
            }
        }
        
        Err(anyhow!("File not found: {}", path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smb_client_creation() {
        let client = SmbClient::new(
            "192.168.1.1".to_string(),
            "user".to_string(),
            "pass".to_string(),
            Some("share".to_string()),
        );
        
        assert_eq!(client.host, "192.168.1.1");
        assert_eq!(client.username, "user");
        assert_eq!(client.password, "pass");
        assert_eq!(client.share, "share");
    }

    #[test]
    fn test_smb_client_default_share() {
        let client = SmbClient::new(
            "192.168.1.1".to_string(),
            "user".to_string(),
            "pass".to_string(),
            None,
        );
        
        assert_eq!(client.share, "share");
    }


    #[test]
    fn test_parse_list_line() {
        let client = SmbClient::new(
            "192.168.1.1".to_string(),
            "user".to_string(),
            "pass".to_string(),
            None,
        );
        
        // Test directory entry
        let dir_line = "  Documents                         D        0  Wed Dec 25 10:30:45 2024";
        let result = client.parse_list_line(dir_line, "/");
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.name, "Documents");
        assert!(entry.is_dir);
        assert_eq!(entry.size, 0);
        
        // Test file entry
        let file_line = "  report.pdf                        A     1024  Wed Dec 25 10:30:45 2024";
        let result = client.parse_list_line(file_line, "/docs");
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.name, "report.pdf");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 1024);
        assert_eq!(entry.path, "/docs/report.pdf");
    }

    #[test]
    fn test_parse_list_line_skip_dots() {
        let client = SmbClient::new(
            "192.168.1.1".to_string(),
            "user".to_string(),
            "pass".to_string(),
            None,
        );
        
        // Should skip . and .. entries
        assert!(client.parse_list_line(".    D        0  Wed Dec 25 10:30:45 2024", "/").is_none());
        assert!(client.parse_list_line("..   D        0  Wed Dec 25 10:30:45 2024", "/").is_none());
    }

    #[test] 
    fn test_check_smbclient_available() {
        // This test will fail if smbclient is not installed, which is expected
        // In CI environments, we'd install samba-client first
        // For now, just test that the function doesn't panic
        let _result = SmbClient::check_smbclient_available();
    }
}
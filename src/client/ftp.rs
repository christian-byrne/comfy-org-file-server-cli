use super::{FileServerClient, RemoteFile};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Local;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use suppaftp::FtpStream;

pub struct FtpClient {
    host: String,
    username: String,
    password: String,
}

impl FtpClient {
    pub fn new(host: String, username: String, password: String) -> Self {
        Self {
            host,
            username,
            password,
        }
    }
    
    fn connect_ftp(host: &str, username: &str, password: &str) -> Result<FtpStream> {
        let mut ftp = FtpStream::connect(host)?;
        ftp.login(username, password)?;
        Ok(ftp)
    }

    fn parse_list_line(line: &str) -> Option<RemoteFile> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            return None;
        }

        let is_dir = parts[0].starts_with('d');
        let size = parts[4].parse::<u64>().unwrap_or(0);
        let name = parts[8..].join(" ");

        // Parse date (simplified - in production would need better parsing)
        let modified = Local::now(); // TODO: Parse actual date from FTP listing

        Some(RemoteFile {
            name: name.clone(),
            path: name,
            size,
            modified,
            is_dir,
        })
    }
}

#[async_trait]
impl FileServerClient for FtpClient {
    async fn connect(&mut self) -> Result<()> {
        // Test connection
        let host = self.host.clone();
        let username = self.username.clone();
        let password = self.password.clone();

        tokio::task::spawn_blocking(move || {
            let mut ftp = Self::connect_ftp(&host, &username, &password)?;
            ftp.quit()?;
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // Nothing to do - we create fresh connections for each operation
        Ok(())
    }

    async fn list_files(&mut self, path: &str) -> Result<Vec<RemoteFile>> {
        let host = self.host.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let path = path.to_string();

        let files = tokio::task::spawn_blocking(move || {
            let mut ftp = Self::connect_ftp(&host, &username, &password)?;
            ftp.cwd(&path)?;
            let list = ftp.list(None)?;
            ftp.quit()?;

            let files: Vec<RemoteFile> = list
                .iter()
                .filter_map(|line| FtpClient::parse_list_line(line))
                .map(|mut file| {
                    file.path = format!("{}/{}", path.trim_end_matches('/'), file.name);
                    file
                })
                .collect();

            Ok::<_, anyhow::Error>(files)
        })
        .await??;

        Ok(files)
    }

    async fn download_file(&mut self, remote_path: &str, local_path: &Path) -> Result<()> {
        let host = self.host.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let remote_path = remote_path.to_string();
        let local_path = local_path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            let mut ftp = Self::connect_ftp(&host, &username, &password)?;
            let mut reader = ftp.retr_as_buffer(&remote_path)?;
            let mut data = Vec::new();
            reader.read_to_end(&mut data)?;
            ftp.quit()?;

            let mut file = File::create(local_path)?;
            file.write_all(&data)?;
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    async fn upload_file(&mut self, local_path: &Path, remote_path: &str) -> Result<()> {
        let host = self.host.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let remote_path = remote_path.to_string();
        let local_path = local_path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            let mut file = File::open(local_path)?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;

            let mut ftp = Self::connect_ftp(&host, &username, &password)?;
            ftp.put_file(&remote_path, &mut &data[..])?;
            ftp.quit()?;
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    async fn create_directory(&mut self, path: &str) -> Result<()> {
        let host = self.host.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let path = path.to_string();

        tokio::task::spawn_blocking(move || {
            let mut ftp = Self::connect_ftp(&host, &username, &password)?;
            ftp.mkdir(&path)?;
            ftp.quit()?;
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    async fn delete_file(&mut self, path: &str) -> Result<()> {
        let host = self.host.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let path = path.to_string();

        tokio::task::spawn_blocking(move || {
            let mut ftp = Self::connect_ftp(&host, &username, &password)?;
            ftp.rm(&path)?;
            ftp.quit()?;
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    async fn get_file_size(&mut self, path: &str) -> Result<u64> {
        let host = self.host.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let path = path.to_string();

        let size = tokio::task::spawn_blocking(move || {
            let mut ftp = Self::connect_ftp(&host, &username, &password)?;
            let size: Result<u64, anyhow::Error> = match ftp.size(&path) {
                Ok(size) => Ok(size as u64),
                Err(e) => Err(e.into()),
            };
            ftp.quit()?;
            size
        })
        .await??;

        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_list_line_directory() {
        let line = "drwxr-xr-x 2 user group 4096 Nov 15 10:30 Documents";
        let result = FtpClient::parse_list_line(line);

        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.name, "Documents");
        assert!(entry.is_dir);
        assert_eq!(entry.size, 4096);
    }

    #[test]
    fn test_parse_list_line_file() {
        let line = "-rw-r--r-- 1 user group 12345 Nov 15 10:30 test.pdf";
        let result = FtpClient::parse_list_line(line);

        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.name, "test.pdf");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 12345);
    }

    #[test]
    fn test_parse_list_line_with_spaces() {
        let line = "-rw-r--r-- 1 user group 1024 Nov 15 10:30 my file name.txt";
        let result = FtpClient::parse_list_line(line);

        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.name, "my file name.txt");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 1024);
    }

    #[test]
    fn test_parse_list_line_invalid() {
        let line = "invalid line";
        let result = FtpClient::parse_list_line(line);
        assert!(result.is_none());
    }

    #[test]
    fn test_ftp_client_creation() {
        let client = FtpClient::new(
            "192.168.1.1:21".to_string(),
            "user".to_string(),
            "pass".to_string(),
        );

        assert_eq!(client.host, "192.168.1.1:21");
        assert_eq!(client.username, "user");
        assert_eq!(client.password, "pass");
    }
}

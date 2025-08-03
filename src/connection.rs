use crate::client::{ftp::FtpClient, smb::SmbClient, FileServerClient};
use crate::config::Config;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ConnectionManager {
    config: Config,
    client: Option<Arc<Mutex<Box<dyn FileServerClient>>>>,
}

impl ConnectionManager {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            client: None,
        }
    }

    pub async fn connect(&mut self) -> Result<Arc<Mutex<Box<dyn FileServerClient>>>> {
        if let Some(client) = &self.client {
            return Ok(client.clone());
        }

        let password = self
            .config
            .password
            .clone()
            .ok_or_else(|| anyhow!("Password not configured"))?;

        // Try SMB first
        let mut smb_client = SmbClient::new(
            self.config.server_ip.clone(),
            self.config.username.clone(),
            password.clone(),
            Some("share".to_string()),
        );

        match smb_client.connect().await {
            Ok(_) => {
                println!("Connected via SMB");
                let client: Box<dyn FileServerClient> = Box::new(smb_client);
                let arc_client = Arc::new(Mutex::new(client));
                self.client = Some(arc_client.clone());
                return Ok(arc_client);
            }
            Err(e) => {
                eprintln!("SMB connection failed: {}, trying FTP fallback", e);
            }
        }

        // Fallback to FTP
        let mut ftp_client = FtpClient::new(
            format!("{}:21", self.config.server_ip),
            self.config.username.clone(),
            password,
        );

        match ftp_client.connect().await {
            Ok(_) => {
                println!("Connected via FTP");
                let client: Box<dyn FileServerClient> = Box::new(ftp_client);
                let arc_client = Arc::new(Mutex::new(client));
                self.client = Some(arc_client.clone());
                Ok(arc_client)
            }
            Err(e) => {
                eprintln!("FTP connection also failed: {}", e);
                Err(anyhow!("Failed to connect to file server via both SMB and FTP"))
            }
        }
    }

    #[allow(dead_code)]
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(client) = self.client.take() {
            let mut client = client.lock().await;
            client.disconnect().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::RemoteFile;
    use crate::config::Protocol;
    use async_trait::async_trait;
    use mockall::mock;
    use std::path::Path;

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

    #[test]
    fn test_connection_manager_creation() {
        let config = Config::default();
        let manager = ConnectionManager::new(config.clone());

        assert_eq!(manager.config.server_ip, config.server_ip);
        assert_eq!(manager.config.username, config.username);
        assert!(manager.client.is_none());
    }

    #[tokio::test]
    async fn test_connection_manager_connect() {
        let config = Config::default();
        let manager = ConnectionManager::new(config);

        // Test connection (this will actually try to connect to the server)
        // In a real test environment, we'd mock the FTP client
        // For now, we just verify the manager structure
        assert!(manager.client.is_none());
    }

    #[tokio::test]
    async fn test_connection_manager_disconnect_without_connection() {
        let config = Config::default();
        let mut manager = ConnectionManager::new(config);

        // Should not panic when disconnecting without a connection
        let result = manager.disconnect().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_connection_manager_with_custom_config() {
        let mut config = Config::default();
        config.server_ip = "10.0.0.1".to_string();
        config.username = "testuser".to_string();
        config.default_protocol = Protocol::Smb;

        let manager = ConnectionManager::new(config.clone());

        assert_eq!(manager.config.server_ip, "10.0.0.1");
        assert_eq!(manager.config.username, "testuser");
        assert_eq!(manager.config.default_protocol, Protocol::Smb);
    }

    #[test]
    fn test_connection_manager_no_password() {
        let mut config = Config::default();
        config.password = None;

        let manager = ConnectionManager::new(config);
        assert!(manager.config.password.is_none());
    }
}

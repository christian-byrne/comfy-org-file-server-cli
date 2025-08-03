use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server_ip: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: Option<String>,
    pub default_protocol: Protocol,
    #[serde(default)]
    pub configured: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Protocol {
    Ftp,
    Smb,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_ip: String::new(),
            username: String::new(),
            password: None,
            default_protocol: Protocol::Ftp,
            configured: false,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        if let Some(config_path) = Self::config_path() {
            if config_path.exists() {
                let content = fs::read_to_string(config_path)?;
                let config: Config = serde_json::from_str(&content)?;
                return Ok(config);
            }
        }
        Ok(Self::default())
    }

    pub fn save(&self) -> Result<()> {
        if let Some(config_path) = Self::config_path() {
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(self)?;
            fs::write(config_path, content)?;
        }
        Ok(())
    }

    fn config_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "comfy", "comfy-fs")
            .map(|dirs| dirs.config_dir().join("config.json"))
    }

    pub fn is_configured(&self) -> bool {
        self.configured && !self.server_ip.is_empty() && !self.username.is_empty()
    }

    pub fn interactive_setup(&mut self) -> Result<()> {
        println!("\n🚀 Welcome to Comfy File Server CLI!");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("\nThis tool helps you access the company file server easily.");
        println!("\n📖 You can find the server credentials (IP, username, password) at:");
        println!("\x1b[36mhttps://www.notion.so/comfy-org/File-Server-Guide-2436d73d3650803f8aedcb7d2177d347?source=copy_link\x1b[0m\n");
        println!("Let's configure your connection settings:\n");

        // Get server IP
        print!("Server IP address: ");
        io::stdout().flush()?;
        let mut server_ip = String::new();
        io::stdin().read_line(&mut server_ip)?;
        self.server_ip = server_ip.trim().to_string();

        // Get username
        print!("Username: ");
        io::stdout().flush()?;
        let mut username = String::new();
        io::stdin().read_line(&mut username)?;
        self.username = username.trim().to_string();

        // Get password with hidden input
        self.password = Some(rpassword::prompt_password("Password: ").unwrap_or_default());

        // Get preferred protocol
        print!("\nPreferred protocol (1=SMB, 2=FTP) [default: 1]: ");
        io::stdout().flush()?;
        let mut protocol_choice = String::new();
        io::stdin().read_line(&mut protocol_choice)?;
        
        self.default_protocol = match protocol_choice.trim() {
            "2" => Protocol::Ftp,
            _ => Protocol::Smb,
        };

        self.configured = true;

        println!("\n✅ Configuration complete!");
        println!("Your settings have been saved to: {:?}", Self::config_path());
        println!("\nYou can reconfigure at any time by running: comfy-fs config\n");

        self.save()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server_ip, "");
        assert_eq!(config.username, "");
        assert_eq!(config.password, None);
        assert_eq!(config.default_protocol, Protocol::Ftp);
        assert_eq!(config.configured, false);
    }

    #[test]
    fn test_protocol_serialization() {
        let ftp = Protocol::Ftp;
        let smb = Protocol::Smb;

        let ftp_json = serde_json::to_string(&ftp).unwrap();
        let smb_json = serde_json::to_string(&smb).unwrap();

        assert_eq!(ftp_json, "\"Ftp\"");
        assert_eq!(smb_json, "\"Smb\"");

        let ftp_decoded: Protocol = serde_json::from_str(&ftp_json).unwrap();
        let smb_decoded: Protocol = serde_json::from_str(&smb_json).unwrap();

        assert_eq!(ftp_decoded, Protocol::Ftp);
        assert_eq!(smb_decoded, Protocol::Smb);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            server_ip: "10.0.0.1".to_string(),
            username: "testuser".to_string(),
            password: Some("testpass".to_string()),
            default_protocol: Protocol::Smb,
            configured: true,
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("10.0.0.1"));
        assert!(json.contains("testuser"));
        assert!(!json.contains("testpass")); // Password should be skipped
        assert!(json.contains("Smb"));

        let decoded: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.server_ip, "10.0.0.1");
        assert_eq!(decoded.username, "testuser");
        assert_eq!(decoded.password, None); // Password not serialized
        assert_eq!(decoded.default_protocol, Protocol::Smb);
        assert_eq!(decoded.configured, true);
    }

    #[test]
    fn test_config_save_and_load() {
        // This test would need to mock the config directory
        // For now, just test that the methods compile
        let _config = Config::default();
        // In a real test, we'd use a temp directory
        // let temp_dir = TempDir::new().unwrap();
        // ... mock ProjectDirs to use temp_dir
    }

    #[test]
    fn test_is_configured() {
        let mut config = Config::default();
        assert!(!config.is_configured());

        config.server_ip = "192.168.1.1".to_string();
        assert!(!config.is_configured());

        config.username = "user".to_string();
        assert!(!config.is_configured());

        config.configured = true;
        assert!(config.is_configured());

        config.server_ip = String::new();
        assert!(!config.is_configured());
    }
}

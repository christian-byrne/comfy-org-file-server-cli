#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::uninlined_format_args,
    clippy::cast_precision_loss,
    clippy::significant_drop_tightening
)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use std::io;
use std::path::PathBuf;

mod browser;
mod client;
mod config;
mod connection;
mod download;
mod utils;

use browser::FileBrowser;
use config::Config;
use utils::glob_match;

/// Helper function to ensure config has password, prompting if needed
fn ensure_password(config: &mut Config) -> Result<()> {
    if config.password.is_none() {
        use std::io::Write;
        
        print!("Password (hidden - you won't see it when you type): ");
        std::io::stdout().flush()?;
        
        // Try to read password securely, fallback to regular input if needed
        match rpassword::prompt_password("") {
            Ok(password) => {
                config.password = Some(password);
            }
            Err(_) => {
                // Fallback to regular input if secure input fails
                let mut password = String::new();
                std::io::stdin().read_line(&mut password)?;
                config.password = Some(password.trim().to_string());
            }
        }
    }
    Ok(())
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Upload files to the server
    Upload {
        /// Files to upload
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Destination directory on server
        #[arg(short, long)]
        dest: Option<String>,
    },

    /// Download files from the server
    Download {
        /// Remote path (supports wildcards)
        path: String,

        /// Local destination directory
        #[arg(short, long, default_value = ".")]
        dest: PathBuf,
    },

    /// List files on the server
    List {
        /// Directory to list
        #[arg(default_value = "/")]
        path: String,

        /// Sort by: modified (default), name, size, type
        #[arg(short, long, default_value = "modified")]
        sort: String,

        /// Reverse sort order
        #[arg(short, long)]
        reverse: bool,
    },

    /// Browse server files interactively
    Browse {
        /// Starting directory
        #[arg(default_value = "/")]
        path: String,
    },

    /// Sync a local directory with the server
    Sync {
        /// Local directory
        local: PathBuf,

        /// Remote directory
        remote: String,
    },

    /// Interactive TUI mode
    Interactive,

    /// Configure server settings
    Config {
        /// Server IP address
        #[arg(long)]
        server: Option<String>,

        /// Username
        #[arg(long)]
        username: Option<String>,

        /// Password (will prompt if not provided)
        #[arg(long)]
        password: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Check if we need to run first-time setup
    let mut config = Config::load()?;
    if !config.is_configured() && !matches!(cli.command, Some(Commands::Config { .. })) {
        config.interactive_setup()?;
    }

    match cli.command {
        None => {
            // No command provided, launch interactive TUI
            browse_mode("/".to_string()).await?;
        }
        Some(Commands::Interactive) => {
            browse_mode("/".to_string()).await?;
        }
        Some(Commands::Browse { path }) => {
            browse_mode(path).await?;
        }
        Some(Commands::Upload { files, dest }) => {
            let mut config = Config::load()?;
            ensure_password(&mut config)?;
            let mut conn_mgr = connection::ConnectionManager::new(config);
            let client = conn_mgr.connect().await?;

            let dest_path = dest.unwrap_or_else(|| "/".to_string());

            println!("Uploading {} files to {}", files.len(), dest_path);

            let mut successful = 0;
            let mut failed = 0;

            for file in files {
                if !file.exists() {
                    eprintln!("File not found: {:?}", file);
                    failed += 1;
                    continue;
                }

                let filename = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");

                let remote_path = format!("{}/{}", dest_path.trim_end_matches('/'), filename);

                print!("Uploading {:?} to {} ... ", file, remote_path);

                let mut client_guard = client.lock().await;
                match client_guard.upload_file(&file, &remote_path).await {
                    Ok(_) => {
                        println!("✓");
                        successful += 1;
                    }
                    Err(e) => {
                        println!("✗ Error: {}", e);
                        failed += 1;
                    }
                }
            }

            println!(
                "\nUpload complete: {} successful, {} failed",
                successful, failed
            );
        }
        Some(Commands::Download { path, dest }) => {
            let mut config = Config::load()?;
            ensure_password(&mut config)?;
            let mut conn_mgr = connection::ConnectionManager::new(config);
            let client = conn_mgr.connect().await?;

            // Check if path contains wildcards
            if path.contains('*') {
                // Handle wildcard download
                let dir = path.split('*').next().unwrap_or("/");
                let pattern = path.split('/').last().unwrap_or("*");

                let mut client_guard = client.lock().await;
                let files = client_guard.list_files(dir).await?;
                drop(client_guard);

                // Filter files based on pattern
                let matching_files: Vec<_> = files
                    .into_iter()
                    .filter(|f| !f.is_dir && glob_match(&f.name, pattern))
                    .map(|f| (f.path.clone(), dest.join(&f.name)))
                    .collect();

                if matching_files.is_empty() {
                    println!("No files match pattern: {}", pattern);
                    return Ok(());
                }

                println!(
                    "Downloading {} files matching '{}'",
                    matching_files.len(),
                    pattern
                );

                let downloader = download::ParallelDownloader::new(client, 4);
                let results = downloader.download_files(matching_files).await?;

                let successful = results.iter().filter(|r| r.is_ok()).count();
                println!(
                    "Downloaded {}/{} files successfully",
                    successful,
                    results.len()
                );
            } else {
                // Single file download
                let filename = path.split('/').last().unwrap_or("file");
                let local_path = dest.join(filename);

                println!("Downloading {} to {:?}", path, local_path);

                let mut client_guard = client.lock().await;
                client_guard.download_file(&path, &local_path).await?;

                println!("Download complete!");
            }
        }
        Some(Commands::List {
            path,
            sort: _,
            reverse: _,
        }) => {
            let mut config = Config::load()?;
            ensure_password(&mut config)?;
            let mut conn_mgr = connection::ConnectionManager::new(config);
            let client = conn_mgr.connect().await?;
            let mut client = client.lock().await;

            let files = client.list_files(&path).await?;
            println!("Files in {}:", path);
            println!("{:<50} {:>10} {:>20}", "Name", "Size", "Modified");
            println!("{}", "-".repeat(80));

            for file in files {
                let size_str = if file.is_dir {
                    "DIR".to_string()
                } else {
                    human_bytes::human_bytes(file.size as f64).to_string()
                };
                println!(
                    "{:<50} {:>10} {:>20}",
                    file.name,
                    size_str,
                    file.modified.format("%Y-%m-%d %H:%M:%S")
                );
            }
        }
        Some(Commands::Sync { local, remote }) => {
            let mut config = Config::load()?;
            ensure_password(&mut config)?;
            let mut conn_mgr = connection::ConnectionManager::new(config);
            let client = conn_mgr.connect().await?;

            println!("Syncing {:?} with {}", local, remote);

            // Get list of remote files
            let mut client_guard = client.lock().await;
            let remote_files = client_guard.list_files(&remote).await?;
            drop(client_guard);

            // Get list of local files
            let mut local_files = std::collections::HashMap::new();
            if local.exists() && local.is_dir() {
                for entry in std::fs::read_dir(&local)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                            let metadata = entry.metadata()?;
                            local_files.insert(filename.to_string(), (path, metadata.len()));
                        }
                    }
                }
            } else {
                tokio::fs::create_dir_all(&local).await?;
            }

            // Download files that don't exist locally or are different sizes
            let mut to_download = Vec::new();
            for remote_file in remote_files.iter().filter(|f| !f.is_dir) {
                if let Some((_, local_size)) = local_files.get(&remote_file.name) {
                    if *local_size != remote_file.size {
                        to_download.push((remote_file.path.clone(), local.join(&remote_file.name)));
                    }
                } else {
                    to_download.push((remote_file.path.clone(), local.join(&remote_file.name)));
                }
            }

            if !to_download.is_empty() {
                println!("Downloading {} files...", to_download.len());
                let downloader = download::ParallelDownloader::new(client.clone(), 4);
                let results = downloader.download_files(to_download).await?;
                let successful = results.iter().filter(|r| r.is_ok()).count();
                println!("Downloaded {}/{} files", successful, results.len());
            }

            // Upload files that don't exist remotely
            let remote_names: std::collections::HashSet<_> = remote_files
                .iter()
                .filter(|f| !f.is_dir)
                .map(|f| f.name.clone())
                .collect();

            let mut to_upload = Vec::new();
            for (name, (path, _)) in local_files {
                if !remote_names.contains(&name) {
                    to_upload.push((path, format!("{}/{}", remote.trim_end_matches('/'), name)));
                }
            }

            if !to_upload.is_empty() {
                println!("Uploading {} files...", to_upload.len());
                let mut successful = 0;
                for (local_path, remote_path) in to_upload {
                    let mut client_guard = client.lock().await;
                    if client_guard
                        .upload_file(&local_path, &remote_path)
                        .await
                        .is_ok()
                    {
                        successful += 1;
                    }
                }
                println!("Uploaded {} files", successful);
            }

            println!("Sync complete!");
        }
        Some(Commands::Config {
            server,
            username,
            password,
        }) => {
            let mut config = Config::load()?;

            // If no arguments provided, run interactive setup
            if server.is_none() && username.is_none() && password.is_none() {
                config.interactive_setup()?;
            } else {
                // Update only the provided fields
                if let Some(server) = server {
                    config.server_ip = server;
                }
                if let Some(username) = username {
                    config.username = username;
                }
                if let Some(password) = password {
                    config.password = Some(password);
                }
                config.configured = true;

                config.save()?;
                println!("Configuration saved successfully!");
            }
        }
    }

    Ok(())
}


async fn browse_mode(start_path: String) -> Result<()> {
    // Connect to server
    let mut config = Config::load()?;
    ensure_password(&mut config)?;
    
    let mut conn_mgr = connection::ConnectionManager::new(config);
    let client = conn_mgr.connect().await?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the browser
    let mut browser = FileBrowser::new(start_path, client);
    let res = browser.run(&mut terminal).await;

    // Restore terminal
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}


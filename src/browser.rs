use anyhow::Result;
use chrono::{DateTime, Local};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::client::FileServerClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: DateTime<Local>,
    pub is_dir: bool,
    pub extension: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Modified,
    Name,
    Size,
    Type,
}

impl SortMode {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn next(&self) -> Self {
        match self {
            Self::Modified => Self::Name,
            Self::Name => Self::Size,
            Self::Size => Self::Type,
            Self::Type => Self::Modified,
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Modified => "Modified",
            Self::Name => "Name",
            Self::Size => "Size",
            Self::Type => "Type",
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct FileBrowser {
    current_path: String,
    entries: Vec<FileEntry>,
    selected: usize,
    sort_mode: SortMode,
    reverse_sort: bool,
    list_state: ListState,
    selected_files: Vec<String>,
    client: Arc<Mutex<Box<dyn FileServerClient>>>,
}

impl FileBrowser {
    pub fn new(start_path: String, client: Arc<Mutex<Box<dyn FileServerClient>>>) -> Self {
        Self {
            current_path: start_path,
            entries: Vec::new(),
            selected: 0,
            sort_mode: SortMode::Modified,
            reverse_sort: false,
            list_state: ListState::default(),
            selected_files: Vec::new(),
            client,
        }
    }

    #[allow(clippy::future_not_send)]
    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        // Load initial directory
        self.load_directory()?;

        loop {
            terminal.draw(|f| self.render(f))?;

            if let Event::Key(key) = event::read()? {
                match self.handle_input(key).await {
                    Ok(false) => break,
                    Ok(true) => continue,
                    Err(e) => {
                        // Show error in status bar
                        eprintln!("Error: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // File list
                Constraint::Length(3), // Status bar
            ])
            .split(frame.area());

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                "Comfy File Browser",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" - "),
            Span::styled(&self.current_path, Style::default().fg(Color::Yellow)),
        ]))
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, chunks[0]);

        // File list
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(_idx, entry)| {
                let is_selected = self.selected_files.contains(&entry.path);
                let style = if entry.is_dir {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default()
                };

                let mut spans = vec![];

                // Selection indicator
                if is_selected {
                    spans.push(Span::styled("[x] ", Style::default().fg(Color::Green)));
                } else {
                    spans.push(Span::raw("[ ] "));
                }

                // File/Dir icon
                if entry.is_dir {
                    spans.push(Span::styled("📁 ", style));
                } else {
                    spans.push(Span::styled("📄 ", style));
                }

                // Name
                spans.push(Span::styled(&entry.name, style));

                // Size (for files)
                if !entry.is_dir {
                    let size_str = format_bytes(entry.size);
                    spans.push(Span::raw(format!(" ({})", size_str)));
                }

                // Modified date
                let modified = entry.modified.format("%Y-%m-%d %H:%M").to_string();
                spans.push(Span::styled(
                    format!(" - {}", modified),
                    Style::default().fg(Color::DarkGray),
                ));

                ListItem::new(Line::from(spans))
            })
            .collect();

        let files_list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(format!(
                "Files [Sort: {} {}]",
                self.sort_mode.as_str(),
                if self.reverse_sort { "↑" } else { "↓" }
            )))
            .highlight_style(Style::default().bg(Color::DarkGray));

        self.list_state.select(Some(self.selected));
        frame.render_stateful_widget(files_list, chunks[1], &mut self.list_state);

        // Status bar
        let status = Paragraph::new(Line::from(vec![Span::raw(
            "↑↓: Navigate | Enter: Open/Download | Space: Select | s: Sort | r: Reverse | q: Quit",
        )]))
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(status, chunks[2]);
    }

    async fn handle_input(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(false),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Enter => self.enter_selected().await?,
            KeyCode::Char(' ') => self.toggle_selection(),
            KeyCode::Char('s') => self.cycle_sort_mode(),
            KeyCode::Char('r') => self.toggle_reverse_sort(),
            KeyCode::Backspace => self.go_up().await?,
            _ => {}
        }
        Ok(true)
    }

    fn move_selection(&mut self, delta: i32) {
        if self.entries.is_empty() {
            return;
        }

        let new_selected = (self.selected as i32 + delta)
            .max(0)
            .min(self.entries.len() as i32 - 1) as usize;
        self.selected = new_selected;
    }

    async fn enter_selected(&mut self) -> Result<()> {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.is_dir {
                self.current_path = entry.path.clone();
                self.selected = 0;
                self.load_directory()?;
            } else {
                // TODO: Implement download
                println!("Download: {}", entry.path);
            }
        }
        Ok(())
    }

    fn toggle_selection(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            if self.selected_files.contains(&entry.path) {
                self.selected_files.retain(|p| p != &entry.path);
            } else {
                self.selected_files.push(entry.path.clone());
            }
        }
    }

    fn cycle_sort_mode(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.sort_entries();
    }

    fn toggle_reverse_sort(&mut self) {
        self.reverse_sort = !self.reverse_sort;
        self.sort_entries();
    }

    async fn go_up(&mut self) -> Result<()> {
        if self.current_path != "/" {
            if let Some(parent) = PathBuf::from(&self.current_path).parent() {
                self.current_path = parent.to_string_lossy().to_string();
                self.selected = 0;
                self.load_directory()?;
            }
        }
        Ok(())
    }

    fn load_directory(&mut self) -> Result<()> {
        // Load files from server
        let client = self.client.clone();
        let path = self.current_path.clone();
        
        let rt = tokio::runtime::Handle::current();
        let remote_files = rt.block_on(async {
            let mut client_guard = client.lock().await;
            client_guard.list_files(&path).await
        })?;

        // Convert RemoteFile to FileEntry
        self.entries = remote_files
            .into_iter()
            .map(|rf| FileEntry {
                name: rf.name.clone(),
                path: rf.path.clone(),
                size: rf.size,
                modified: rf.modified,
                is_dir: rf.is_dir,
                extension: if rf.is_dir {
                    None
                } else {
                    rf.name
                        .rfind('.')
                        .map(|i| rf.name[i + 1..].to_string())
                },
            })
            .collect();

        self.sort_entries();
        Ok(())
    }

    fn sort_entries(&mut self) {
        self.entries.sort_by(|a, b| {
            // Directories always come first
            if a.is_dir != b.is_dir {
                return if a.is_dir {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }

            let ordering = match self.sort_mode {
                SortMode::Modified => b.modified.cmp(&a.modified), // Newest first by default
                SortMode::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortMode::Size => b.size.cmp(&a.size),
                SortMode::Type => {
                    let a_ext = a.extension.as_deref().unwrap_or("");
                    let b_ext = b.extension.as_deref().unwrap_or("");
                    a_ext.cmp(b_ext)
                }
            };

            if self.reverse_sort {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
}

fn format_bytes(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < units.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, units[unit_idx])
    } else {
        format!("{:.1} {}", size, units[unit_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_mode_cycle() {
        assert_eq!(SortMode::Modified.next(), SortMode::Name);
        assert_eq!(SortMode::Name.next(), SortMode::Size);
        assert_eq!(SortMode::Size.next(), SortMode::Type);
        assert_eq!(SortMode::Type.next(), SortMode::Modified);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
        assert_eq!(format_bytes(1099511627776), "1.0 TB");
    }

    #[test]
    fn test_toggle_selection() {
        use crate::client::{RemoteFile, FileServerClient};
        use async_trait::async_trait;
        use std::path::Path;
        
        // Create a mock client
        struct MockClient;
        
        #[async_trait]
        impl FileServerClient for MockClient {
            async fn connect(&mut self) -> Result<()> { Ok(()) }
            async fn disconnect(&mut self) -> Result<()> { Ok(()) }
            async fn list_files(&mut self, _path: &str) -> Result<Vec<RemoteFile>> { Ok(vec![]) }
            async fn download_file(&mut self, _remote_path: &str, _local_path: &Path) -> Result<()> { Ok(()) }
            async fn upload_file(&mut self, _local_path: &Path, _remote_path: &str) -> Result<()> { Ok(()) }
            async fn create_directory(&mut self, _path: &str) -> Result<()> { Ok(()) }
            async fn delete_file(&mut self, _path: &str) -> Result<()> { Ok(()) }
            async fn get_file_size(&mut self, _path: &str) -> Result<u64> { Ok(0) }
        }
        
        let client: Arc<Mutex<Box<dyn FileServerClient>>> = Arc::new(Mutex::new(Box::new(MockClient)));
        let mut browser = FileBrowser::new("/".to_string(), client);
        browser.entries = vec![FileEntry {
            name: "test.txt".to_string(),
            path: "/test.txt".to_string(),
            size: 100,
            modified: Local::now(),
            is_dir: false,
            extension: Some("txt".to_string()),
        }];

        assert!(browser.selected_files.is_empty());

        browser.toggle_selection();
        assert_eq!(browser.selected_files.len(), 1);
        assert_eq!(browser.selected_files[0], "/test.txt");

        browser.toggle_selection();
        assert!(browser.selected_files.is_empty());
    }
}

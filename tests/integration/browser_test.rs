use comfy_fs::browser::{FileBrowser, FileEntry, SortMode};
use rstest::*;
use serial_test::serial;
use test_case::test_case;

mod common;
use common::*;

#[test_case("/" ; "root directory")]
#[test_case("/Documents" ; "subdirectory")]
#[serial]
async fn test_browser_initialization(start_path: &str) {
    let browser = FileBrowser::new(start_path.to_string());
    // Verify browser starts with expected defaults
    assert_eq!(browser.sort_mode, SortMode::Modified);
    assert!(!browser.reverse_sort);
    assert_eq!(browser.selected, 0);
}

#[rstest]
#[serial]
async fn test_full_browser_workflow(
    test_config: TestConfig,
    mock_files: Vec<MockFileEntry>,
) {
    // This would test the full browser workflow with mocked server responses
    // Including navigation, sorting, and selection
    
    // TODO: Implement once we have the file server client
}
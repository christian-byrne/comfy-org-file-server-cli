// Utility functions extracted for testing

pub fn glob_match(filename: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if let Some(ext) = pattern.strip_prefix("*.") {
        filename.ends_with(&format!(".{}", ext))
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        filename.starts_with(prefix)
    } else if let Some(suffix) = pattern.strip_prefix('*') {
        filename.ends_with(suffix)
    } else {
        filename == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_wildcard() {
        assert!(glob_match("anything.txt", "*"));
        assert!(glob_match("test.pdf", "*"));
        assert!(glob_match("", "*"));
    }

    #[test]
    fn test_glob_match_extension() {
        assert!(glob_match("file.txt", "*.txt"));
        assert!(glob_match("document.pdf", "*.pdf"));
        assert!(glob_match("archive.tar.gz", "*.gz"));
        assert!(!glob_match("file.txt", "*.pdf"));
        assert!(!glob_match("file", "*.txt"));
    }

    #[test]
    fn test_glob_match_prefix() {
        assert!(glob_match("test_file.txt", "test*"));
        assert!(glob_match("test_document.pdf", "test*"));
        assert!(glob_match("test", "test*"));
        assert!(!glob_match("file_test.txt", "test*"));
        assert!(!glob_match("atest", "test*"));
    }

    #[test]
    fn test_glob_match_suffix() {
        assert!(glob_match("file_test", "*test"));
        assert!(glob_match("document_test", "*test"));
        assert!(glob_match("test", "*test"));
        assert!(!glob_match("test_file", "*test"));
        assert!(!glob_match("testa", "*test"));
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("exact.txt", "exact.txt"));
        assert!(glob_match("test", "test"));
        assert!(!glob_match("exact.txt", "other.txt"));
        assert!(!glob_match("test", "test2"));
    }

    #[test]
    fn test_glob_match_edge_cases() {
        assert!(glob_match("", ""));
        assert!(!glob_match("file", ""));
        assert!(!glob_match("", "pattern"));
        assert!(glob_match("*", "*"));
        assert!(glob_match("*", "**")); // "*" matches "**" because "*" matches everything after the initial "*"
        assert!(!glob_match("file", "**file")); // Double wildcard isn't supported
    }
}
pub const TRACKED_DIRECTORIES: [&str; 2] = ["wiki", "raw"];

pub fn is_tracked(path: &str) -> bool {
    TRACKED_DIRECTORIES
        .iter()
        .any(|tracked| path == *tracked || path.starts_with(&format!("{tracked}/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_workspace_scope_is_tracked() {
        assert!(is_tracked("wiki/index.md"));
        assert!(is_tracked("raw/source.md"));
        assert!(!is_tracked(".wiki-db/corpusbot.sqlite3"));
        assert!(!is_tracked("../outside.md"));
    }
}

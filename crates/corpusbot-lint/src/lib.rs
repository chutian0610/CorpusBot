pub const RESERVED_PAGE_PATHS: [&str; 2] = ["wiki/index.md", "wiki/log.md"];

pub fn is_reserved_page(path: &str) -> bool {
    RESERVED_PAGE_PATHS.contains(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_catalog_and_log_are_reserved() {
        assert!(is_reserved_page("wiki/index.md"));
        assert!(is_reserved_page("wiki/log.md"));
        assert!(!is_reserved_page("wiki/entities/rust.md"));
    }
}

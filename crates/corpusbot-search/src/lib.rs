pub const SEARCH_INDEX_DIR: &str = ".wiki-db/tantivy";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_state_stays_engine_private() {
        assert_eq!(SEARCH_INDEX_DIR, ".wiki-db/tantivy");
    }
}
pub mod error;
pub mod index;

pub use error::{Result, SearchError};
pub use index::{SearchDocument, SearchGeneration, SearchHit, SearchIndex};

pub const MAX_ANALYSIS_ATTEMPTS: u32 = 2;
pub const MAX_DRAFT_ATTEMPTS: u32 = 2;
pub mod error;
pub mod service;

pub use error::{IngestError, Result};
pub use service::{IngestResult, Ingestor};

pub fn attempts_exhausted(attempt: u32, max: u32) -> bool {
    attempt >= max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_loops_are_bounded() {
        assert!(!attempts_exhausted(1, MAX_ANALYSIS_ATTEMPTS));
        assert!(attempts_exhausted(2, MAX_ANALYSIS_ATTEMPTS));
        assert!(attempts_exhausted(2, MAX_DRAFT_ATTEMPTS));
    }
}

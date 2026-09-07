pub mod error;
pub mod template;

pub use error::{CoreError, Result};
pub use template::Template;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn library_version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_the_workspace_version() {
        assert_eq!(library_version(), "0.1.0");
    }
}

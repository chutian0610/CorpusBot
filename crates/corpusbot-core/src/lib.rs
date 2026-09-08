pub mod date;
pub mod document;
pub mod error;
pub mod frontmatter;
pub mod identity;
pub mod link;
pub mod page_type;
pub mod path;
pub mod resource;
pub mod source;
pub mod template;

pub use date::IsoDate;
pub use document::WikiDoc;
pub use error::{CoreError, Result};
pub use frontmatter::Frontmatter;
pub use identity::PageIdentity;
pub use link::Wikilink;
pub use page_type::PageType;
pub use path::WikiPath;
pub use resource::{ManifestId, ResourceId, ResourceRevision, Revision, RevisionManifest};
pub use source::{SourceRecord, SourceRef};
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

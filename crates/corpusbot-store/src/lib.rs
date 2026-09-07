pub const ENGINE_DIR: &str = ".wiki-db";

pub fn engine_dir_name() -> &'static str {
    ENGINE_DIR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_state_is_private_to_the_workspace() {
        assert_eq!(engine_dir_name(), ".wiki-db");
    }
}

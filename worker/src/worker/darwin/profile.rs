// TODO: improve profile with more granular permissions
pub const STDENV_DEFAULT: &str = r#"
(version 1)
(allow default)
(allow process-exec)
(allow process-fork)
"#;

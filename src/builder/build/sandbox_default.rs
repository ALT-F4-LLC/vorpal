pub const SANDBOX_DEFAULT: &str = r#"
(version 1)

; Global allows
(allow default)
(allow process-exec)
(allow process-fork)

; Global denies
(deny network*)
"#;

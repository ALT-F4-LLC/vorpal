use crate::artifact::SYSTEMS;
use anyhow::Result;
use indoc::formatdoc;
use std::fmt::Write;
use vorpal_sdk::{artifact::Job, context::ConfigContext};

const SECRET_NAME: &str = "SYNTHETIC_SECRET";

/// Prefix of the synthetic value. The `test-secret-argv` CI job searches host process
/// arguments for this exact string, so a change here must be mirrored in its `MARKER`.
const MARKER: &str = "vorpal-synthetic-secret-c4d9e2f1";

/// SHA-256 of `synthetic_value()`. The step compares against this digest so the
/// value itself has one carrier: the secret file.
const VALUE_SHA256: &str = "fdf32dd01e78a8844f6060c776abdb856b4d6dd50226793f8cf90f38d6ee87d9";

/// Non-secret argument of the step's long-running process, which lets the CI watcher
/// prove it can see processes inside the build sandbox.
const VISIBLE_TOKEN: &str = "vorpal-secret-job-visible";

/// Printed by the step once the secret file matches `VALUE_SHA256`. The `test-secret-argv`
/// CI job requires this line in the build log, so a change here must be mirrored there.
const RETRIEVED_MESSAGE: &str = "secret retrieved: SYNTHETIC_SECRET matches expected digest";

fn synthetic_value() -> String {
    let mut value = format!("{MARKER}=");

    for line in 0..32 {
        // Writing to a `String` cannot fail.
        let _ = write!(value, "synthetic-line-{line:02}=not-a-real-credential;");
    }

    value
}

fn script() -> String {
    formatdoc! {r#"
        digest=$(sha256sum "$VORPAL_SECRET_DIR/{SECRET_NAME}")
        if [[ "${{digest%% *}}" != "{VALUE_SHA256}" ]]; then
            echo "secret file does not hold the expected synthetic value" >&2
            exit 1
        fi
        echo "{RETRIEVED_MESSAGE}"
        bash -c 'sleep 15; :' {VISIBLE_TOKEN}
    "#}
}

#[derive(Default)]
pub struct VorpalSecretJob {}

impl VorpalSecretJob {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        Job::new("vorpal-secret-job", script(), SYSTEMS.to_vec())
            .with_secrets(vec![(SECRET_NAME, &synthetic_value())])
            .build(context)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::{
        script, synthetic_value, MARKER, RETRIEVED_MESSAGE, SECRET_NAME, VALUE_SHA256,
        VISIBLE_TOKEN,
    };

    #[test]
    fn synthetic_value_is_long_contains_equals_and_starts_with_marker() {
        let value = synthetic_value();

        assert!(value.len() >= 1024, "value is {} bytes", value.len());
        assert!(value.contains('='));
        assert!(value.starts_with(MARKER));
    }

    #[test]
    fn synthetic_value_avoids_credential_shapes() {
        let value = synthetic_value();

        for shape in [
            "PRIVATE KEY",
            "AKIA",
            "ghp_",
            "gho_",
            "ghs_",
            "github_pat_",
            "xox",
            "sk-ant-",
            "AIza",
        ] {
            assert!(!value.contains(shape), "value contains {shape:?}");
        }
    }

    #[test]
    fn script_compares_by_digest_and_never_carries_the_value() {
        let script = script();

        assert!(script.contains(VALUE_SHA256));
        assert!(script.contains(&format!("\"$VORPAL_SECRET_DIR/{SECRET_NAME}\"")));
        assert!(!script.contains(&synthetic_value()));
        assert!(!script.contains(MARKER));
        assert!(!script.contains(&format!("${SECRET_NAME}")));
        assert!(!script.contains("VORPAL_OUTPUT"));
    }

    #[test]
    fn script_confirms_retrieval_only_after_the_digest_check() {
        let script = script();
        let check = script.find(VALUE_SHA256).expect("digest check");
        let confirm = script
            .find(&format!("echo \"{RETRIEVED_MESSAGE}\""))
            .expect("retrieval confirmation");

        assert!(confirm > check);
        assert!(RETRIEVED_MESSAGE.contains(SECRET_NAME));
        assert!(!RETRIEVED_MESSAGE.contains(MARKER));
    }

    #[test]
    fn script_runs_a_visible_process_for_several_poll_intervals() {
        assert!(script().contains(&format!("bash -c 'sleep 15; :' {VISIBLE_TOKEN}")));
    }
}

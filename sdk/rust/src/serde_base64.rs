//! Serde adapter for a protobuf `bytes` field: writes the standard, padded
//! base64 alphabet (RFC 4648 section 4) and reads back only that canonical
//! spelling, so each value has exactly one JSON form and one digest.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{de::Error as _, Deserialize, Deserializer, Serializer};

pub(crate) fn serialize<S: Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&STANDARD.encode(value))
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let encoded = String::deserialize(deserializer)?;

    // The decode error names the offending input byte, so it is not echoed:
    // the value can be a secret.
    STANDARD
        .decode(encoded)
        .map_err(|_| D::Error::custom("value is not canonical padded base64"))
}

#[cfg(test)]
mod tests {
    use crate::api::artifact::{
        Artifact, ArtifactSource, ArtifactStep, ArtifactStepSecret, ArtifactSystem,
    };

    const SHARED_VECTOR: [u8; 5] = [0xff, 0x00, 0xfe, 0x3d, 0x0a];

    fn artifact_with_secrets(secrets: Vec<ArtifactStepSecret>) -> Artifact {
        Artifact {
            aliases: vec!["example:latest".to_string()],
            name: "example".to_string(),
            sources: vec![ArtifactSource {
                digest: Some("abc123".to_string()),
                excludes: vec!["target".to_string()],
                includes: vec!["src".to_string()],
                name: "example".to_string(),
                path: ".".to_string(),
            }],
            steps: vec![ArtifactStep {
                arguments: vec!["-c".to_string()],
                artifacts: vec!["def456".to_string()],
                entrypoint: Some("bash".to_string()),
                environments: vec!["PATH=/bin".to_string()],
                script: Some("echo hi".to_string()),
                secrets,
            }],
            systems: vec![ArtifactSystem::Aarch64Darwin.into()],
            target: ArtifactSystem::Aarch64Darwin.into(),
        }
    }

    fn artifact_digest(artifact: &Artifact) -> anyhow::Result<String> {
        Ok(sha256::digest(serde_json::to_vec(artifact)?))
    }

    #[test]
    fn secret_value_serializes_as_padded_standard_base64() -> anyhow::Result<()> {
        let secret = ArtifactStepSecret {
            name: "TOKEN".to_string(),
            value: SHARED_VECTOR.to_vec(),
        };

        assert_eq!(
            serde_json::to_string(&secret)?,
            r#"{"name":"TOKEN","value":"/wD+PQo="}"#
        );

        Ok(())
    }

    #[test]
    fn secret_value_rejects_non_canonical_base64() {
        for json in [
            r#"{"name":"TOKEN","value":"/wD+PQo"}"#,
            r#"{"name":"TOKEN","value":"_wD-PQo="}"#,
            r#"{"name":"TOKEN","value":"/wD+PQp="}"#,
        ] {
            assert!(
                serde_json::from_str::<ArtifactStepSecret>(json).is_err(),
                "accepted {json}"
            );
        }
    }

    #[test]
    fn previous_release_artifact_json_parses_to_decoded_bytes() -> anyhow::Result<()> {
        // Written by the previous release, where `value` was a string holding
        // the agent's base64 ciphertext text.
        let stored = r#"{"target":1,"sources":[],"steps":[{"entrypoint":"bash","script":"echo hi","secrets":[{"name":"TOKEN","value":"aHVudGVyMg=="}],"arguments":[],"artifacts":[],"environments":[]}],"systems":[1],"aliases":[],"name":"example"}"#;

        let artifact: Artifact = serde_json::from_str(stored)?;

        assert_eq!(artifact.steps[0].secrets[0].value, b"hunter2");
        assert_eq!(serde_json::to_string(&artifact)?, stored);

        Ok(())
    }

    #[test]
    fn artifact_without_secrets_keeps_its_digest() -> anyhow::Result<()> {
        // Digest computed from this artifact before `value` became `bytes`.
        assert_eq!(
            artifact_digest(&artifact_with_secrets(vec![]))?,
            "71bd330295891ae4a37684e7eb10f12bfbfb6fa0058db0f357c00b747fd752fc"
        );

        Ok(())
    }

    mod abuse {
        use super::*;

        #[test]
        fn non_utf8_secret_value_round_trips_and_keeps_a_distinct_digest() -> anyhow::Result<()> {
            assert!(String::from_utf8(SHARED_VECTOR.to_vec()).is_err());

            let secret = |value: &[u8]| ArtifactStepSecret {
                name: "TOKEN".to_string(),
                value: value.to_vec(),
            };
            let artifact = artifact_with_secrets(vec![secret(&SHARED_VECTOR)]);
            let other = artifact_with_secrets(vec![secret(&[0xfe, 0x00, 0xff, 0x3d, 0x0a])]);

            let parsed: Artifact = serde_json::from_slice(&serde_json::to_vec(&artifact)?)?;

            assert_eq!(parsed, artifact);
            assert_ne!(artifact_digest(&artifact)?, artifact_digest(&other)?);

            Ok(())
        }
    }
}

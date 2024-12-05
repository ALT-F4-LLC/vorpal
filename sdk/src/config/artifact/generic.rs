use crate::{
    artifact::linux::{debian, vorpal},
    service::ContextConfig,
    steps::{bash, bwrap, env_artifact},
};
use anyhow::{bail, Result};
use std::path::Path;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId, ArtifactSource,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};
use vorpal_store::{hashes::hash_files, paths::get_file_paths};



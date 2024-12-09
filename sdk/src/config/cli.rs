use crate::config::ConfigContext;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env::consts::{ARCH, OS};
use tracing::Level;
use vorpal_schema::{get_artifact_system, vorpal::artifact::v0::ArtifactSystem};

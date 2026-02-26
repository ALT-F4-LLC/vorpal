use vorpal_sdk::api::artifact::{
    ArtifactSystem,
    ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
};

pub const SYSTEMS: [ArtifactSystem; 4] = [Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

pub mod vorpal;
pub mod vorpal_container_image;
pub mod vorpal_job;
pub mod vorpal_process;
pub mod vorpal_release;
pub mod vorpal_shell;
pub mod vorpal_user;

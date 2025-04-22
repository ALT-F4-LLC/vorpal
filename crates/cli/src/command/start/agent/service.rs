use crate::command::start::agent::build_source;
use anyhow::Result;
use sha256::digest;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use vorpal_sdk::api::{
    agent::{agent_service_server::AgentService, PrepareArtifactRequest, PrepareArtifactResponse},
    artifact::{Artifact, ArtifactSource},
};

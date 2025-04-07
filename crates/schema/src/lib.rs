pub mod agent {
    pub mod v0 {
        tonic::include_proto!("vorpal.agent.v0");
    }
}

pub mod archive {
    pub mod v0 {
        tonic::include_proto!("vorpal.archive.v0");
    }
}

pub mod artifact {
    pub mod v0 {
        tonic::include_proto!("vorpal.artifact.v0");
    }
}

pub mod worker {
    pub mod v0 {
        tonic::include_proto!("vorpal.worker.v0");
    }
}

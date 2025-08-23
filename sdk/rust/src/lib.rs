pub mod api {
    pub mod agent {
        tonic::include_proto!("vorpal.agent");
    }

    pub mod archive {
        tonic::include_proto!("vorpal.archive");
    }

    pub mod artifact {
        tonic::include_proto!("vorpal.artifact");
    }

    pub mod context {
        tonic::include_proto!("vorpal.context");
    }

    pub mod worker {
        tonic::include_proto!("vorpal.worker");
    }
}

pub mod artifact;
pub mod auth;
pub mod cli;
pub mod context;

use anyhow::Result;
use sha256::digest;
use std::collections::HashMap;
use tonic::transport::Server;
use vorpal_schema::vorpal::{
    config::v0::{
        config_service_server::{ConfigService, ConfigServiceServer},
        Config, ConfigRequest,
    },
    package::v0::{Package, PackageOutput},
};

#[derive(Debug, Default)]
pub struct ContextConfig {
    package: HashMap<String, Package>,
}

impl ContextConfig {
    pub fn add_package(&mut self, package: Package) -> Result<PackageOutput> {
        let package_json = serde_json::to_string(&package).map_err(|e| anyhow::anyhow!(e))?;

        let package_hash = digest(package_json.as_bytes());

        let package_key = format!("{}-{}", package.name, package_hash);

        if !self.package.contains_key(&package_key) {
            self.package.insert(package_key.clone(), package.clone());
        }

        let package_output = PackageOutput {
            hash: package_hash,
            name: package.name,
        };

        Ok(package_output)
    }

    pub fn get_package(&self, hash: &str, name: &str) -> Option<&Package> {
        let package_key = format!("{}-{}", name, hash);

        self.package.get(&package_key)
    }
}

#[derive(Debug, Default)]
pub struct ConfigServer {
    pub context: ContextConfig,
    pub config: Config,
}

impl ConfigServer {
    pub fn new(context: ContextConfig, config: Config) -> Self {
        Self { context, config }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigServer {
    async fn get_config(
        &self,
        _request: tonic::Request<ConfigRequest>,
    ) -> Result<tonic::Response<Config>, tonic::Status> {
        Ok(tonic::Response::new(self.config.clone()))
    }

    async fn get_package(
        &self,
        request: tonic::Request<PackageOutput>,
    ) -> Result<tonic::Response<Package>, tonic::Status> {
        let request = request.into_inner();

        let package = self
            .context
            .get_package(request.hash.as_str(), request.name.as_str());

        if package.is_none() {
            return Err(tonic::Status::not_found("Package input not found"));
        }

        Ok(tonic::Response::new(package.unwrap().clone()))
    }
}

pub async fn listen(context: ContextConfig, config: Config, port: u16) -> Result<()> {
    let addr = format!("[::]:{}", port)
        .parse()
        .expect("failed to parse address");

    let config_service = ConfigServiceServer::new(ConfigServer::new(context, config));

    Server::builder()
        .add_service(config_service)
        .serve(addr)
        .await
        .expect("failed to start worker server");

    Ok(())
}

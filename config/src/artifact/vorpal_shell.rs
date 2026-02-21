use crate::artifact::SYSTEMS;
use anyhow::Result;
use vorpal_sdk::artifact::{
    bun::Bun,
    crane::Crane,
    go::Go,
    goimports::Goimports,
    gopls::Gopls,
    grpcurl::Grpcurl,
    language::go::{get_goarch, get_goos},
    nodejs::NodeJS,
    pnpm::Pnpm,
    protoc::Protoc,
    protoc_gen_go::ProtocGenGo,
    protoc_gen_go_grpc::ProtocGenGoGrpc,
    staticcheck::Staticcheck,
};
use vorpal_sdk::{artifact, context::ConfigContext};

#[derive(Default)]
pub struct VorpalShell {}

impl VorpalShell {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let bun = Bun::new().build(context).await?;
        let crane = Crane::new().build(context).await?;
        let go = Go::new().build(context).await?;
        let goimports = Goimports::new().build(context).await?;
        let gopls = Gopls::new().build(context).await?;
        let grpcurl = Grpcurl::new().build(context).await?;
        let nodejs = NodeJS::new().build(context).await?;
        let pnpm = Pnpm::new().build(context).await?;
        let protoc = Protoc::new().build(context).await?;
        let protoc_gen_go = ProtocGenGo::new().build(context).await?;
        let protoc_gen_go_grpc = ProtocGenGoGrpc::new().build(context).await?;
        let staticcheck = Staticcheck::new().build(context).await?;

        artifact::ProjectEnvironment::new("vorpal-shell", SYSTEMS.to_vec())
            .with_artifacts(vec![
                bun,
                crane,
                go,
                goimports,
                gopls,
                grpcurl,
                nodejs,
                pnpm,
                protoc,
                protoc_gen_go,
                protoc_gen_go_grpc,
                staticcheck,
            ])
            .with_environments(vec![
                "CGO_ENABLED=0".to_string(),
                format!("GOARCH={}", get_goarch(context.get_system())?),
                format!("GOOS={}", get_goos(context.get_system())?),
            ])
            .with_secrets(vec![])
            .build(context)
            .await
    }
}

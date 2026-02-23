use crate::artifact::SYSTEMS;
use anyhow::Result;
use vorpal_sdk::{
    artifact::{
        get_env_key, language::typescript::TypeScriptLibrary, protoc_gen_ts_proto::ProtocGenTsProto,
    },
    context::ConfigContext,
};

#[derive(Default)]
pub struct VorpalSdkTypescript {}

impl VorpalSdkTypescript {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let proto_artifact = ProtocGenTsProto::new().build(context).await?;
        let proto_env = get_env_key(&proto_artifact);

        TypeScriptLibrary::new("vorpal-sdk-typescript", SYSTEMS.to_vec())
            .with_includes(vec!["sdk/typescript"])
            .with_artifacts(vec![proto_artifact])
            .with_aliases(vec!["vorpal-sdk-typescript:latest".to_string()])
            .with_source_script(format!("cd sdk/typescript\ncp -pr {proto_env}/api src/api"))
            .build(context)
            .await
    }
}

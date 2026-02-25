use crate::artifact::SYSTEMS;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_sdk::{
    artifact::{
        get_env_key, language::typescript::TypeScript, protoc_gen_ts_proto::ProtocGenTsProto,
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

        TypeScript::new("vorpal-sdk-typescript", SYSTEMS.to_vec())
            .with_aliases(vec!["vorpal-sdk-typescript:latest".to_string()])
            .with_artifacts(vec![proto_artifact])
            .with_includes(vec!["sdk/typescript"])
            .with_source_scripts(vec![formatdoc! {"
                cd sdk/typescript
                cp -pr {proto_env}/api src/api
            "}])
            .with_vorpal_sdk(false)
            .build(context)
            .await
    }
}

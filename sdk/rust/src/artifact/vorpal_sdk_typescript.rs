use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{
        get_env_key, language::typescript::TypeScript,
        vorpal_sdk_typescript_proto::VorpalSdkTypeScriptProto,
    },
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

#[derive(Default)]
pub struct VorpalSdkTypescript {}

impl VorpalSdkTypescript {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let proto = VorpalSdkTypeScriptProto::new().build(context).await?;
        let proto_env = get_env_key(&proto);
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        TypeScript::new("vorpal-sdk-typescript", systems)
            .with_artifacts(vec![proto])
            .with_includes(vec!["sdk/typescript"])
            .with_source_scripts(vec![formatdoc! {"
                cd sdk/typescript
                cp -pr {proto_env}/api src/api
            "}])
            .build(context)
            .await
    }
}

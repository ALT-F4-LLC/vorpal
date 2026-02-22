use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{bun::Bun, get_env_key, protoc::Protoc, step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

/// Pinned version of the `ts-proto` npm package.
///
/// This MUST match the version in `sdk/typescript/package.json` devDependencies
/// to ensure the generated TypeScript protobuf files are byte-identical to what
/// developers produce locally with `bun run generate:proto`.
const TS_PROTO_VERSION: &str = "2.11.2";

#[derive(Default)]
pub struct ProtocGenTsProto {}

impl ProtocGenTsProto {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "protoc-gen-ts-proto";

        // Build dependencies
        let protoc = Protoc::new().build(context).await?;
        let bun = Bun::new().build(context).await?;

        let protoc_bin = format!("{}/bin", get_env_key(&protoc));
        let bun_bin = format!("{}/bin", get_env_key(&bun));

        // Source: .proto files from sdk/rust/api/
        let source = ArtifactSource::new(name, ".")
            .with_includes(vec!["sdk/rust/api".to_string()])
            .build();

        let source_dir = format!("./source/{name}/sdk/rust/api");

        let step_script = formatdoc! {r#"
            mkdir -p "$VORPAL_OUTPUT/api"

            # Install ts-proto (provides protoc-gen-ts_proto plugin)
            cd $VORPAL_WORKSPACE
            echo '{{"private":true}}' > $VORPAL_WORKSPACE/package.json
            {bun_bin}/bun add ts-proto@{TS_PROTO_VERSION}

            # Create node symlink so the protoc-gen-ts_proto shebang (#!/usr/bin/env node) works
            mkdir -p $VORPAL_WORKSPACE/bin
            ln -s {bun_bin}/bun $VORPAL_WORKSPACE/bin/node

            # Run protoc with ts-proto plugin
            PATH="$VORPAL_WORKSPACE/bin:$PATH" \
            {protoc_bin}/protoc \
              --plugin=protoc-gen-ts_proto=$VORPAL_WORKSPACE/node_modules/.bin/protoc-gen-ts_proto \
              --ts_proto_out=$VORPAL_OUTPUT/api \
              --ts_proto_opt=outputServices=grpc-js \
              --ts_proto_opt=esModuleInterop=true \
              --ts_proto_opt=snakeToCamel=false \
              --ts_proto_opt=forceLong=number \
              --ts_proto_opt=useOptionals=messages \
              --ts_proto_opt=oneof=unions \
              --ts_proto_opt=env=node \
              --ts_proto_opt=importSuffix=.js \
              -I {source_dir} \
              {source_dir}/agent/agent.proto \
              {source_dir}/archive/archive.proto \
              {source_dir}/artifact/artifact.proto \
              {source_dir}/context/context.proto \
              {source_dir}/worker/worker.proto"#,
        };

        let steps =
            vec![step::shell(context, vec![protoc, bun], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_aliases(vec![format!("{name}:{TS_PROTO_VERSION}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}

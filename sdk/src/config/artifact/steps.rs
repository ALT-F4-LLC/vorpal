use crate::config::artifact::get_artifact_envkey;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{ArtifactEnvironment, ArtifactId, ArtifactStep};

// TODO: implement cache for sources

pub fn bash(environments: Vec<ArtifactEnvironment>, script: String) -> ArtifactStep {
    ArtifactStep {
        arguments: vec![],
        entrypoint: None,
        environments,
        script: Some(formatdoc! {"
            #!/bin/bash
            set -euo pipefail

            {script}",
            script = script,
        }),
    }
}

pub fn bwrap(
    arguments: Vec<String>,
    artifacts: Vec<ArtifactId>,
    environments: Vec<ArtifactEnvironment>,
    rootfs: Option<String>,
    script: String,
) -> ArtifactStep {
    let mut args = vec![
        "--unshare-all".to_string(),
        "--share-net".to_string(),
        "--clearenv".to_string(),
        "--chdir".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        "--gid".to_string(),
        "1000".to_string(),
        "--uid".to_string(),
        "1000".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
        "--bind".to_string(),
        "$VORPAL_OUTPUT".to_string(),
        "$VORPAL_OUTPUT".to_string(),
        "--bind".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        "--setenv".to_string(),
        "VORPAL_OUTPUT".to_string(),
        "$VORPAL_OUTPUT".to_string(),
        "--setenv".to_string(),
        "VORPAL_WORKSPACE".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
    ];

    if let Some(rootfs) = rootfs {
        args = [
            args,
            vec![
                // mount bin
                "--ro-bind".to_string(),
                format!("{}/bin", rootfs),
                "/bin".to_string(),
                // mount etc
                "--ro-bind".to_string(),
                format!("{}/etc", rootfs),
                "/etc".to_string(),
                // mount lib
                "--ro-bind".to_string(),
                format!("{}/lib", rootfs),
                "/lib".to_string(),
                // mount lib64 (if exists)
                "--ro-bind-try".to_string(),
                format!("{}/lib64", rootfs),
                "/lib64".to_string(),
                // mount sbin
                "--ro-bind".to_string(),
                format!("{}/sbin", rootfs),
                "/sbin".to_string(),
                // mount usr
                "--ro-bind".to_string(),
                format!("{}/usr", rootfs),
                "/usr".to_string(),
            ],
        ]
        .concat();
    }

    for artifact in artifacts {
        // add read-only mounts
        args.push("--ro-bind".to_string());
        args.push(get_artifact_envkey(&artifact));
        args.push(get_artifact_envkey(&artifact));

        // add environment variables
        args.push("--setenv".to_string());
        args.push(get_artifact_envkey(&artifact).replace("$", ""));
        args.push(get_artifact_envkey(&artifact));
    }

    for env in environments.clone() {
        args.push("--setenv".to_string());
        args.push(env.key.clone());
        args.push(env.value.clone());
    }

    for arg in arguments {
        args.push(arg);
    }

    // TODO: use amber instead of bash as a proof of concept

    bash(
        environments,
        formatdoc! {"
            cat > $VORPAL_WORKSPACE/bwrap.sh << \"EOS\"
            #!/bin/bash
            set -euo pipefail

            {script}
            EOS

            chmod +x $VORPAL_WORKSPACE/bwrap.sh

            {entrypoint} {arguments} $VORPAL_WORKSPACE/bwrap.sh",
            entrypoint = "/usr/bin/bwrap",
            arguments = args.join(" "),
        },
    )
}

pub fn docker(arguments: Vec<String>) -> ArtifactStep {
    bash(vec![], format!("/usr/bin/docker {}", arguments.join(" ")))
}

use crate::{
    api::artifact::ArtifactSystem::{Aarch64Linux, X8664Linux},
    artifact::{get_env_key, step, Artifact},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

fn generate_version_script() -> String {
    formatdoc! {"
        #!/bin/bash
        # A script to list version numbers of critical development tools

        # If you have tools installed in other directories, adjust PATH here AND
        # in ~lfs/.bashrc (section 4.4) as well.

        LC_ALL=C
        PATH=/usr/bin:/bin

        bail() {{ echo \"FATAL: $1\"; exit 1; }}
        grep --version > /dev/null 2> /dev/null || bail \"grep does not work\"
        sed '' /dev/null || bail \"sed does not work\"
        sort   /dev/null || bail \"sort does not work\"

        ver_check()
        {{
           if ! type -p $2 &>/dev/null
           then
             echo \"ERROR: Cannot find $2 ($1)\"; return 1;
           fi
           v=$($2 --version 2>&1 | grep -E -o '[0-9]+\\.[0-9\\.]+[a-z]*' | head -n1)
           if printf '%s\\n' $3 $v | sort --version-sort --check &>/dev/null
           then
             printf \"OK:    %-9s %-6s >= $3\\n\" \"$1\" \"$v\"; return 0;
           else
             printf \"ERROR: %-9s is TOO OLD ($3 or later required)\\n\" \"$1\";
             return 1;
           fi
        }}

        ver_kernel()
        {{
           kver=$(uname -r | grep -E -o '^[0-9\\.]+')
           if printf '%s\\n' $1 $kver | sort --version-sort --check &>/dev/null
           then
             printf \"OK:    Linux Kernel $kver >= $1\\n\"; return 0;
           else
             printf \"ERROR: Linux Kernel ($kver) is TOO OLD ($1 or later required)\\n\" \"$kver\";
             return 1;
           fi
        }}

        # Coreutils first because --version-sort needs Coreutils >= 7.0
        ver_check Coreutils      sort     8.1 || bail \"Coreutils too old, stop\"
        ver_check Bash           bash     3.2
        ver_check Binutils       ld       2.13.1
        ver_check Bison          bison    2.7
        ver_check Diffutils      diff     2.8.1
        ver_check Findutils      find     4.2.31
        ver_check Gawk           gawk     4.0.1
        ver_check GCC            gcc      5.2
        ver_check \"GCC (C++)\"  g++      5.2
        ver_check Grep           grep     2.5.1a
        ver_check Gzip           gzip     1.3.12
        ver_check M4             m4       1.4.10
        ver_check Make           make     4.0
        ver_check Patch          patch    2.5.4
        ver_check Perl           perl     5.8.8
        ver_check Python         python3  3.4
        ver_check Sed            sed      4.1.5
        ver_check Tar            tar      1.22
        ver_check Texinfo        texi2any 5.0
        ver_check Xz             xz       5.0.0
        ver_kernel 4.19

        if mount | grep -q 'devpts on /dev/pts' && [ -e /dev/ptmx ]
        then echo \"OK:    Linux Kernel supports UNIX 98 PTY\";
        else echo \"ERROR: Linux Kernel does NOT support UNIX 98 PTY\"; fi

        alias_check() {{
           if $1 --version 2>&1 | grep -qi $2
           then printf \"OK:    %-4s is $2\\n\" \"$1\";
           else printf \"ERROR: %-4s is NOT $2\\n\" \"$1\"; fi
        }}
        echo \"Aliases:\"
        alias_check awk GNU
        alias_check yacc Bison
        alias_check sh Bash

        echo \"Compiler check:\"
        if printf \"int main(){{}}\" | g++ -x c++ -
        then echo \"OK:    g++ works\";
        else echo \"ERROR: g++ does NOT work\"; fi
        rm -f a.out

        if [ \"$(nproc)\" = \"\" ]; then
           echo \"ERROR: nproc is not available or it produces empty output\"
        else
           echo \"OK: nproc reports $(nproc) logical cores are available\"
        fi
    "}
}

fn generate_dockerfile() -> String {
    formatdoc! {"
        FROM docker.io/library/debian:sid-slim@sha256:c0f1b3716686ee452f7c62c82d8aee5f79feccba7402e967b79658100d5bd6cf

        RUN ARCH=$(uname -m) \
            && if [ \"${{ARCH}}\" = \"aarch64\" ]; then ARCH=\"arm64\"; fi \
            && if [ \"${{ARCH}}\" = \"x86_64\" ]; then ARCH=\"amd64\"; fi \
            && echo \"Current architecture: ${{ARCH}}\" \
            && apt-get update \
            && apt-get install --yes \
            bash \
            binutils \
            bison \
            bubblewrap \
            bzip2 \
            ca-certificates \
            coreutils \
            curl \
            diffutils \
            g++ \
            gawk \
            gcc \
            grep \
            gzip \
            linux-headers-$ARCH \
            m4 \
            make \
            patch \
            perl \
            python3 \
            rsync \
            sed \
            tar \
            texinfo \
            xz-utils \
            zstd \
            && rm -rf /var/lib/apt/lists/*

        RUN ln -sf /bin/bash /bin/sh \
            && [ ! -e /etc/bash.bashrc ] || mv -v /etc/bash.bashrc /etc/bash.bashrc.NOUSE \
            && groupadd --gid 1000 vorpal \
            && useradd -s /bin/bash -g vorpal -u 1000 -m -k /dev/null vorpal

        USER vorpal

        WORKDIR /home/vorpal

        COPY --chmod=755 --chown=vorpal:vorpal version_check.sh version_check.sh

        RUN ./version_check.sh
    "}
}

#[derive(Default)]
pub struct LinuxDebian {}

impl LinuxDebian {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let systems = vec![Aarch64Linux, X8664Linux];

        let steps = vec![step::bash(
            vec![],
            vec![],
            vec![],
            formatdoc! {"
                cat > $VORPAL_OUTPUT/version_check.sh << \"EOF\"
                {version_script}
                EOF

                cat > $VORPAL_OUTPUT/Dockerfile << \"EOF\"
                {dockerfile}
                EOF",
                dockerfile = generate_dockerfile(),
                version_script = generate_version_script(),
            },
        )];

        let dockerfile = Artifact::new("linux-debian-dockerfile", steps, systems.clone())
            .build(context)
            .await?;

        let image = format!("altf4llc/debin:{dockerfile}");

        let steps = vec![
            step::docker(
                vec![
                    "buildx",
                    "build",
                    "--progress=plain",
                    format!("--tag={image}").as_str(),
                    &get_env_key(&dockerfile),
                ],
                vec![dockerfile.clone()],
            ),
            step::docker(
                vec!["container", "create", "--name", &dockerfile, &image],
                vec![],
            ),
            step::docker(
                vec![
                    "container",
                    "export",
                    "--output",
                    "$VORPAL_WORKSPACE/debian.tar",
                    &dockerfile,
                ],
                vec![],
            ),
            step::bash(
                vec![],
                vec![],
                vec![],
                formatdoc! {"
                    ## extract files
                    tar -xvf $VORPAL_WORKSPACE/debian.tar -C $VORPAL_OUTPUT

                    ## patch files
                    echo \"nameserver 1.1.1.1\" > $VORPAL_OUTPUT/etc/resolv.conf
                "},
            ),
            step::docker(vec!["container", "stop", &dockerfile], vec![]),
            step::docker(vec!["container", "rm", "--force", &dockerfile], vec![]),
        ];

        let name = "linux-debian";

        Artifact::new(name, steps, systems)
            .with_aliases(vec![format!("{name}:latest")])
            .build(context)
            .await
    }
}

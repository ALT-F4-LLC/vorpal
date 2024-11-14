use crate::{package::build_package, ContextConfig};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub fn package(context: &mut ContextConfig) -> Result<PackageOutput> {
    let name = "zlib";

    let package = Package {
        environments: vec![],
        name: name.to_string(),
        packages: vec![],
        sandbox: None,
        script: formatdoc! {"
            pushd ./zlib

            ./configure \
                --prefix=\"$output/usr\"

            make
            make check
            make install

            rm -fv $output/usr/lib/libz.a
        "},
        sources: vec![],
        // source: vec![PackageSource {
        //     excludes: vec![],
        //     hash: Some(
        //         "3f7995d5f103719283f509c23624287ce95c349439e881ed935a3c2c807bb683".to_string(),
        //     ),
        //     includes: vec![],
        //     name: name.to_string(),
        //     strip_prefix: true,
        //     uri: "https://zlib.net/fossils/zlib-1.3.1.tar.gz".to_string(),
        // }],
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    build_package(context, package)
}

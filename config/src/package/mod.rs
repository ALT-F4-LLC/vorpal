use crate::cross_platform::get_sed_cmd;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::Package;

pub mod bash;
pub mod glibc;

pub fn build_package(package: Package) -> Result<Package> {
    let mut script = package.script.clone();

    let script_sanitize = formatdoc! {"
        find $output -type f | while read -r file; do
            if file \"$file\" | grep -q 'text'; then
                {sed} \"s|$output|${envkey}|g\" \"$file\"
                {sed} \"s|$PWD|${envkey}|g\" \"$file\"
            fi
        done

        if [ \"$(uname -s)\" = \"Linux\" ]; then
            find \"$output\" -type f -executable | while read -r file; do
                $patchelf --set-interpreter \"$glibc\" \"$file\"
            done
        fi",
        envkey = package.name.to_lowercase().replace("-", "_"),
        sed = get_sed_cmd()?,
    };

    script.insert("sanitize".to_string(), script_sanitize);

    let package = Package {
        environment: package.environment,
        name: package.name,
        packages: package.packages,
        sandbox: package.sandbox,
        script,
        source: package.source,
        systems: package.systems,
    };

    Ok(package)
}

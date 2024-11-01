use anyhow::{anyhow, bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

#[allow(clippy::too_many_arguments)]
pub fn add_packages(
    package: Package,
    target: PackageSystem,
    bash: Option<&PackageOutput>,
    binutils: Option<&PackageOutput>,
    bison: Option<&PackageOutput>,
    coreutils: Option<&PackageOutput>,
    diffutils: Option<&PackageOutput>,
    file: Option<&PackageOutput>,
    findutils: Option<&PackageOutput>,
    gawk: Option<&PackageOutput>,
    gcc: Option<&PackageOutput>,
    gettext: Option<&PackageOutput>,
    glibc: Option<&PackageOutput>,
    grep: Option<&PackageOutput>,
    gzip: Option<&PackageOutput>,
    libstdcpp: Option<&PackageOutput>,
    linux_headers: Option<&PackageOutput>,
    m4: Option<&PackageOutput>,
    make: Option<&PackageOutput>,
    ncurses: Option<&PackageOutput>,
    patch: Option<&PackageOutput>,
    patchelf: Option<&PackageOutput>,
    perl: Option<&PackageOutput>,
    python: Option<&PackageOutput>,
    sed: Option<&PackageOutput>,
    tar: Option<&PackageOutput>,
    texinfo: Option<&PackageOutput>,
    util_linux: Option<&PackageOutput>,
    xz: Option<&PackageOutput>,
    // zlib: Option<&PackageOutput>,
) -> Result<Package> {
    let mut packages = vec![];

    if target == Aarch64Macos || target == X8664Macos {
        if let Some(bash) = bash {
            packages.push(bash.clone());
        }

        if let Some(coreutils) = coreutils {
            packages.push(coreutils.clone());
        }
    }

    if target == Aarch64Linux || target == X8664Linux {
        if let Some(bash) = bash {
            packages.push(bash.clone());
        }

        if let Some(binutils) = binutils {
            packages.push(binutils.clone());
        }

        if let Some(bison) = bison {
            packages.push(bison.clone());
        }

        if let Some(coreutils) = coreutils {
            packages.push(coreutils.clone());
        }

        if let Some(diffutils) = diffutils {
            packages.push(diffutils.clone());
        }

        if let Some(file) = file {
            packages.push(file.clone());
        }

        if let Some(findutils) = findutils {
            packages.push(findutils.clone());
        }

        if let Some(gawk) = gawk {
            packages.push(gawk.clone());
        }

        if let Some(gcc) = gcc {
            packages.push(gcc.clone());
        }

        if let Some(gettext) = gettext {
            packages.push(gettext.clone());
        }

        if let Some(glibc) = glibc {
            packages.push(glibc.clone());
        }

        if let Some(grep) = grep {
            packages.push(grep.clone());
        }

        if let Some(gzip) = gzip {
            packages.push(gzip.clone());
        }

        if let Some(libstdcpp) = libstdcpp {
            packages.push(libstdcpp.clone());
        }

        if let Some(linux_headers) = linux_headers {
            packages.push(linux_headers.clone());
        }

        if let Some(m4) = m4 {
            packages.push(m4.clone());
        }

        if let Some(make) = make {
            packages.push(make.clone());
        }

        if let Some(ncurses) = ncurses {
            packages.push(ncurses.clone());
        }

        if let Some(patch) = patch {
            packages.push(patch.clone());
        }

        if let Some(patchelf) = patchelf {
            packages.push(patchelf.clone());
        }

        if let Some(perl) = perl {
            packages.push(perl.clone());
        }

        if let Some(python) = python {
            packages.push(python.clone());
        }

        if let Some(sed) = sed {
            packages.push(sed.clone());
        }

        if let Some(tar) = tar {
            packages.push(tar.clone());
        }

        if let Some(texinfo) = texinfo {
            packages.push(texinfo.clone());
        }

        if let Some(util_linux) = util_linux {
            packages.push(util_linux.clone());
        }

        if let Some(xz) = xz {
            packages.push(xz.clone());
        }

        // if let Some(zlib) = zlib {
        //     packages.push(zlib.clone());
        // }
    }

    for package in package.packages {
        packages.push(package);
    }

    let bash = bash.ok_or_else(|| anyhow!("Bash package not found"))?;

    let mut script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail
        export LC_ALL=\"C\"",
        bash = bash.name.to_lowercase().replace("-", "_"),
    };

    if package.script.is_empty() {
        bail!("Package script is empty");
    }

    script.push_str(format!("\n\n{}", package.script).as_str());

    Ok(Package {
        environment: package.environment,
        name: package.name,
        packages,
        sandbox: package.sandbox,
        script,
        source: package.source,
        systems: package.systems,
    })
}

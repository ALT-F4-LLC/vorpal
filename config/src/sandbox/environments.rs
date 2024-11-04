use vorpal_schema::vorpal::package::v0::{Package, PackageEnvironment, PackageOutput};

// pub struct SandboxEnvironment {
//     pub c_include_path: String,
//     pub cppflags: String,
//     pub ld_library_path: String,
//     pub ldflags: String,
//     pub library_path: String,
//     pub package: Package,
// }

// pub fn add_package_environment() {}

// pub struct SandboxEnvironments {
//     pub bash: Option<SandboxEnvironmentsConfig>,
//     pub binutils: Option<SandboxEnvironmentsConfig>,
//     pub gcc: Option<SandboxEnvironmentsConfig>,
//     pub glibc: Option<SandboxEnvironmentsConfig>,
//     pub libstdcpp: Option<SandboxEnvironmentsConfig>,
//     pub linux_headers: Option<SandboxEnvironmentsConfig>,
//     pub ncurses: Option<SandboxEnvironmentsConfig>,
// }

#[allow(clippy::too_many_arguments)]
pub fn add_environments(
    package: Package,
    bash: Option<&PackageOutput>,
    binutils: Option<&PackageOutput>,
    gcc: Option<&PackageOutput>,
    glibc: Option<&PackageOutput>,
    libstdcpp: Option<&PackageOutput>,
    linux_headers: Option<&PackageOutput>,
    ncurses: Option<&PackageOutput>,
    // zlib: Option<&PackageOutput>,
) -> Package {
    let mut c_include_paths = vec![];
    let mut cppflags_args = vec![];
    let mut ld_library_paths = vec![];
    let mut ldflags_args = vec![];
    let mut library_paths = vec![];
    let mut path_paths = vec![];
    // let mut pkg_config_paths = vec![];

    if let Some(bash) = bash {
        let env_key = format!("${}", bash.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);
        let lib_path = format!("{}/lib", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path);
    }

    if let Some(binutils) = binutils {
        let env_key = format!("${}", binutils.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);
        let lib_path = format!("{}/lib", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path.clone());
    }

    if let Some(gcc) = gcc {
        let env_key = format!("${}", gcc.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include/c++/14.2.0", env_key);
        let lib64_path = format!("{}/lib64", env_key);
        let lib_path = format!("{}/lib", env_key);
        let libexec_path = format!("{}/libexec/gcc/aarch64-unknown-linux-gnu/14.2.0", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib64_path.clone());
        ld_library_paths.push(lib_path.clone());
        ld_library_paths.push(libexec_path.clone());
        ldflags_args.push(format!("-L{}", lib64_path));
        ldflags_args.push(format!("-L{}", lib_path));
        ldflags_args.push(format!("-L{}", libexec_path));
        library_paths.push(lib64_path.clone());
        library_paths.push(lib_path.clone());
        library_paths.push(libexec_path.clone());
        path_paths.push(libexec_path.clone());
    }

    if let Some(libstdcpp) = libstdcpp {
        let env_key = format!("${}", libstdcpp.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include/c++/14.2.0", env_key);
        let lib64_path = format!("{}/lib64", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib64_path.clone());
        ldflags_args.push(format!("-L{}", lib64_path));
        library_paths.push(lib64_path.clone());
    }

    if let Some(glibc) = glibc {
        let env_key = format!("${}", glibc.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);
        let lib_path = format!("{}/lib", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        // ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path.clone());
    }

    if let Some(linux_headers) = linux_headers {
        let env_key = format!("${}", linux_headers.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/usr/include", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
    }

    if let Some(ncurses) = ncurses {
        let env_key = format!("${}", ncurses.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include/ncursesw", env_key);
        let lib_path = format!("{}/lib", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path.clone());
    }

    // if let Some(zlib) = zlib {
    //     let env_key = format!("${}", zlib.name.to_lowercase().replace("-", "_"));
    //     let include_path = format!("{}/include", env_key);
    //     let lib_path = format!("{}/lib", env_key);
    //     let pkgconfig_path = format!("{}/lib/pkgconfig", env_key);
    //
    //     c_include_paths.push(include_path.clone());
    //     cppflags_args.push(format!("-I{}", include_path));
    //     ld_library_paths.push(lib_path.clone());
    //     ldflags_args.push(format!("-L{}", lib_path));
    //     library_paths.push(lib_path.clone());
    //
    //     pkg_config_paths.push(pkgconfig_path);
    // }

    let c_include_path_key = "C_INCLUDE_PATH".to_string();
    let cppflags_key = "CPPFLAGS".to_string();
    let ld_library_path_key = "LD_LIBRARY_PATH".to_string();
    let ldflags_key = "LDFLAGS".to_string();
    let library_path_key = "LIBRARY_PATH".to_string();
    let path_key = "PATH".to_string();
    let pkg_config_path_key = "PKG_CONFIG_PATH".to_string();

    let c_include_paths = c_include_paths.join(":");
    let cppflags_args = cppflags_args.join(" ");
    let ld_library_paths = ld_library_paths.join(":");
    let ldflags_args = ldflags_args.join(" ");
    let library_paths = library_paths.join(":");
    let path_paths = path_paths.join(":");
    // let pkg_config_paths = pkg_config_paths.join(":");

    let mut c_include_path = package
        .environment
        .iter()
        .find(|env| env.key == c_include_path_key)
        .unwrap_or(&PackageEnvironment {
            key: c_include_path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut cppflags = package
        .environment
        .iter()
        .find(|env| env.key == cppflags_key)
        .unwrap_or(&PackageEnvironment {
            key: cppflags_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut ldflags = package
        .environment
        .iter()
        .find(|env| env.key == ldflags_key)
        .unwrap_or(&PackageEnvironment {
            key: ldflags_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut ld_library_path = package
        .environment
        .iter()
        .find(|env| env.key == ld_library_path_key)
        .unwrap_or(&PackageEnvironment {
            key: ld_library_path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut library_path = package
        .environment
        .iter()
        .find(|env| env.key == library_path_key)
        .unwrap_or(&PackageEnvironment {
            key: library_path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut path = package
        .environment
        .iter()
        .find(|env| env.key == path_key)
        .unwrap_or(&PackageEnvironment {
            key: path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut pkg_config_path = package
        .environment
        .iter()
        .find(|env| env.key == pkg_config_path_key)
        .unwrap_or(&PackageEnvironment {
            key: pkg_config_path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    if !c_include_path.value.is_empty() {
        c_include_path.value.insert(c_include_path.value.len(), ':');
    }

    if !cppflags.value.is_empty() {
        cppflags.value.insert(cppflags.value.len(), ' ');
    }

    if !ld_library_path.value.is_empty() {
        ld_library_path
            .value
            .insert(ld_library_path.value.len(), ':');
    }

    if !ldflags.value.is_empty() {
        ldflags.value.insert(ldflags.value.len(), ' ');
    }

    if !library_path.value.is_empty() {
        library_path.value.insert(library_path.value.len(), ':');
    }

    if !path.value.is_empty() {
        path.value.insert(path.value.len(), ':');
    }

    if !pkg_config_path.value.is_empty() {
        pkg_config_path
            .value
            .insert(pkg_config_path.value.len(), ':');
    }

    c_include_path
        .value
        .insert_str(c_include_path.value.len(), c_include_paths.as_str());

    cppflags
        .value
        .insert_str(cppflags.value.len(), cppflags_args.as_str());

    ld_library_path
        .value
        .insert_str(ld_library_path.value.len(), ld_library_paths.as_str());

    ldflags
        .value
        .insert_str(ldflags.value.len(), ldflags_args.as_str());

    library_path
        .value
        .insert_str(library_path.value.len(), library_paths.as_str());

    path.value.insert_str(path.value.len(), path_paths.as_str());

    // pkg_config_path
    //     .value
    //     .insert_str(pkg_config_path.value.len(), pkg_config_paths.as_str());

    let mut environment = vec![];

    environment.push(PackageEnvironment {
        key: "LC_ALL".to_string(),
        value: "C".to_string(),
    });

    if let Some(gcc) = gcc {
        let cc_key = "CC".to_string();
        let gcc_key = "GCC".to_string();
        let gcc_env_key = format!("${}", gcc.name.to_lowercase().replace("-", "_"));
        let gcc_path = format!("{}/bin/gcc", gcc_env_key);

        let cc = PackageEnvironment {
            key: cc_key.clone(),
            value: gcc_path.clone(),
        };

        let gcc = PackageEnvironment {
            key: gcc_key.clone(),
            value: gcc_path.clone(),
        };

        environment.push(cc);
        environment.push(gcc);
    }

    environment.push(c_include_path);
    environment.push(cppflags);
    environment.push(ld_library_path);
    environment.push(ldflags);
    environment.push(library_path);
    environment.push(path);
    environment.push(pkg_config_path);

    Package {
        environment,
        name: package.name,
        packages: package.packages,
        sandbox: package.sandbox,
        script: package.script,
        source: package.source,
        systems: package.systems,
    }
}

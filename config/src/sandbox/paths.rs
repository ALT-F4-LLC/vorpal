use vorpal_schema::vorpal::package::v0::PackageSandboxPath;

pub fn add_rootfs() -> Vec<PackageSandboxPath> {
    let root_path = "/vorpal/sandbox-rootfs";

    vec![
        PackageSandboxPath {
            source: format!("{}/bin", root_path),
            symlink: false,
            target: "/bin".to_string(),
        },
        PackageSandboxPath {
            source: format!("{}/etc", root_path),
            symlink: false,
            target: "/etc".to_string(),
        },
        PackageSandboxPath {
            source: format!("{}/lib", root_path),
            symlink: false,
            target: "/lib".to_string(),
        },
        PackageSandboxPath {
            source: format!("{}/usr/lib/x86_64-linux-gnu", root_path),
            symlink: false,
            target: "/lib64".to_string(),
        },
        PackageSandboxPath {
            source: format!("{}/usr", root_path),
            symlink: false,
            target: "/usr".to_string(),
        },
        PackageSandboxPath {
            source: format!("{}/sbin", root_path),
            symlink: false,
            target: "/sbin".to_string(),
        },
        PackageSandboxPath {
            source: format!("{}/var", root_path),
            symlink: false,
            target: "/var".to_string(),
        },
    ]
}

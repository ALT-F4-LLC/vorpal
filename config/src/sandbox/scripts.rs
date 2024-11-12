// use crate::cross_platform::get_sed_cmd;
// use anyhow::{bail, Result};
// use indoc::formatdoc;
// use vorpal_schema::vorpal::package::v0::{
//     Package, PackageOutput, PackageSystem,
//     PackageSystem::{Aarch64Linux, X8664Linux},
// };

// pub struct PackageRpath {
//     pub rpath: String,
//     pub shrink: bool,
//     pub target: String, // TODO: support globs or regex
// }

// pub fn add_scripts(
//     package: Package,
//     system: PackageSystem,
//     glibc: Option<&PackageOutput>,
//     rpaths: Vec<PackageRpath>,
// ) -> Result<Package> {
//     let mut script = package.script.clone();
//
//     let script_paths = formatdoc! {"
//         find \"$output\" -type f | while read -r file; do
//             if file \"$file\" | grep -q 'text'; then
//                 {sed} \"s|$PWD|${envkey}|g\" \"$file\"
//             fi
//         done",
//         envkey = package.name.to_lowercase().replace("-", "_"),
//         sed = get_sed_cmd(system)?,
//     };
//
//     script.push_str(format!("\n\n{}", script_paths).as_str());
//
//     if let Some(glibc) = glibc {
//         let glibc_arch = match system {
//             Aarch64Linux => "aarch64",
//             X8664Linux => "x86_64",
//             _ => bail!("Unsupported interpreter system"),
//         };
//
//         let glibc_script = formatdoc! {"
//             find \"$output\" -type f | while read -r file; do
//                 if file \"$file\" | grep -q 'interpreter /lib/ld-linux-{arch}.so.1'; then
//                     echo \"Patching interpreter: $file -> ${glibc}/lib/ld-linux-{arch}.so.1\"
//
//                     patchelf --set-interpreter \"${glibc}/lib/ld-linux-{arch}.so.1\" \"$file\"
//                 fi
//             done",
//             arch = glibc_arch,
//             glibc = glibc.name.to_lowercase().replace("-", "_"),
//         };
//
//         script.push_str(format!("\n\n{}", glibc_script).as_str());
//     }
//
//     if !rpaths.is_empty() {
//         let rpath_set_script = formatdoc! {"{targets}",
//             targets = rpaths
//                 .iter()
//                 .map(|r| {
//                     formatdoc! {"
//                         set_rpath() {{
//                             local file_path=\"$1\"
//                             local rpath_prev=\"$(patchelf --print-rpath $file_path)\"
//                             local rpath_next=\"\"
//
//                             if [ -n \"$rpath_prev\" ]; then
//                                 rpath_next=\"{rpath}:$rpath_prev\"
//                             else
//                                 rpath_next=\"{rpath}\"
//                             fi
//
//                             echo \"Setting rpath: $file_path -> $rpath_next\"
//
//                             patchelf --set-rpath \"$rpath_next\" \"$file_path\"
//
//                             if [ \"{shrink}\" = \"true\" ]; then
//                                 echo \"Shrinking rpath: $file_path\"
//
//                                 patchelf --shrink-rpath \"$file_path\"
//                             fi
//                         }}
//
//                         target_path=\"{target}\"
//
//                         if [ -f \"$target_path\" ]; then
//                             set_rpath \"$target_path\"
//                         fi
//
//                         if [ -d \"$target_path\" ]; then
//                             find \"$target_path\" -type f | while read -r file; do
//                                 if file \"$file\" | grep -q 'dynamically linked'; then
//                                     set_rpath \"$file\"
//                                 fi
//                             done
//                         fi
//                         ",
//                         rpath = r.rpath,
//                         shrink = if r.shrink { "true" } else { "false" },
//                         target = r.target,
//                     }
//                 })
//                 .collect::<Vec<String>>()
//                 .join("\n"),
//         };
//
//         script.push_str(format!("\n\n{}", rpath_set_script).as_str());
//     }
//
//     Ok(Package {
//         environment: package.environment,
//         name: package.name,
//         packages: package.packages,
//         sandbox: package.sandbox,
//         script,
//         source: package.source,
//         systems: package.systems,
//     })
// }

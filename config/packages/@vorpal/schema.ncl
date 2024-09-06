{
  Config = {
    lockfile | Lock,
    packages | { _ : Package } | default = {},
  },
  LockSandbox = {
    aarch64-linux | String,
    aarch64-macos | String,
    x86_64-linux | String,
    x86_64-macos | String,
  },
  Lock = {
    sandbox | LockSandbox,
  },
  Package = {
    environment | { _ : String } | default = {},
    name | String,
    packages | Array Package | default = [],
    script | String,
    source | String | optional,
    source_excludes | Array String | default = [],
    source_hash | String | optional,
    source_includes | Array String | default = [],
    systems | Array String | default = [],
  },
}

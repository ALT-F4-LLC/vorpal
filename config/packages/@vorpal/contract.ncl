{
  Config = {
    packages | { _ : Package } | default = {},
  },
  PackageSource = {
    excludes | Array String | default = [],
    hash | String | optional,
    includes | Array String | default = [],
    strip_prefix | Bool | default = true,
    uri | String,
  },
  Package = {
    environment | { _ : String } | default = {},
    name | String,
    packages | Array Package | default = [],
    sandbox | Bool | default = true,
    script | String | default
      = m%"
      make check
      make install PREFIX=$output
    "%,
    source | { _ : PackageSource } | default = {},
    systems | Array String | default = [],
  },
}

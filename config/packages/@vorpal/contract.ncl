{
  Config = {
    packages | { _ : Package } | default = {},
  },
  PackageSource = {
    excludes | Array String | default = [],
    hash | String | optional,
    includes | Array String | default = [],
    strip_prefix | Bool | default = false,
    uri | String,
  },
  Package = {
    environment | { _ : String } | default = {},
    name | String,
    packages | Array Package | default = [],
    sandbox | Package | optional,
    script | String | default
      = m%"
        make check
        make install PREFIX=$output
      "%,
    source | { _ : PackageSource } | default = {},
    systems | Array String | default = [],
  },
}

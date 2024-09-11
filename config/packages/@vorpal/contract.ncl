{
  Config = {
    packages | { _ : Package } | default = {},
  },
  Package = {
    environment | { _ : String } | default = {},
    name | String,
    packages | Array Package | default = [],
    script | String | default
      = m%"
      make check
      make install PREFIX=$output
    "%,
    sandbox | Bool | default = true,
    source | String | optional,
    source_excludes | Array String | default = [],
    source_hash | String | optional,
    source_includes | Array String | default = [],
    systems | Array String | default = [],
  },
}

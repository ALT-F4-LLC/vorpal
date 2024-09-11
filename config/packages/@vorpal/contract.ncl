{
  Config = {
    packages | { _ : Package } | default = {},
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
    source | String | optional,
    source_excludes | Array String | default = [],
    source_hash | String | optional,
    source_includes | Array String | default = [],
    systems | Array String | default = [],
  },
}

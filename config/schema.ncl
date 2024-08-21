{
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
    target | String,
  },
  Config = {
    packages | { _ : Package } | default = {},
  },
}

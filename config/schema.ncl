{
  Package = {
    environment | { _ : String } | default = {},
    name | String,
    packages | Array Package | default = [],
    script | String,
    source | String | optional,
    source_hash | String | optional,
    source_ignores | Array String | default = [],
    systems | Array String | default = [],
    target | String,
  },
  Config = {
    packages | { _ : Package } | default = {},
  },
}

{
  Package = {
    environment | { _ : String } | default = {},
    input | { _ : String } | default = {},
    name | String,
    packages | Array Package | default = [],
    script | String,
    source | String | optional,
    source_ignores | Array String | default = [],
    systems | Array String | default = [],
  },
  Config = {
    packages | { _ : Package } | default = {},
  },
}

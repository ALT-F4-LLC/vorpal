let { sandbox } = import "vorpal.lock.ncl" in

{
  Package = {
    environment | { _ : String } | default = {},
    name | String,
    packages | Array Package | default = [],
    sandbox_image | String | default = std.record.get "%{target}" sandbox.image,
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

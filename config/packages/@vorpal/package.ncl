let { Package, .. } = import "@vorpal/contract.ncl" in

fun config =>
  {
    name = config.name,
    packages = config.packages,
    script = m%"
      make check
      make install PREFIX=$output
    "%,
    source = config.source,
    source_excludes = config.source_excludes,
    source_hash = config.source_hash,
    source_includes = config.source_includes,
    systems = config.systems,
  } | Package

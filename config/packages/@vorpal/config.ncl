let { Config, .. } = import "@vorpal/contract.ncl" in

fun config => { packages = config.packages } | Config

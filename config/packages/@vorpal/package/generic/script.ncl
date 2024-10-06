let { Package, .. } = import "@vorpal/contract.ncl" in

let build_bash = import "@vorpal/package/bash.ncl" in

fun target =>
  fun config =>
    {
      name = config.name,
      packages = [build_bash target],
      script = m%"
      mkdir -p $output/bin

      cat << "EOF" > $output/bin/%{config.name}
      #!$bash/bin/bash
      %{config.script}
      EOF

      chmod +x $output/bin/%{config.name}
    "%,
      systems = [
        "aarch64-linux",
        "aarch64-macos",
        "x86_64-linux",
        "x86_64-macos"
      ],
    } | Package

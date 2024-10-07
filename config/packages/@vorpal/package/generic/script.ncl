let build_bash = import "@vorpal/package/bash.ncl" in
let build_package = import "@vorpal/package.ncl" in

fun config =>
  {
    name = config.name,
    packages = [build_bash config.target],
    script | default
      = m%"
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
  }
  |> build_package

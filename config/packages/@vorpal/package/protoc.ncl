let { Package, .. } = import "@vorpal/contract.ncl" in

let build_sandbox = import "@vorpal/package/sandbox.ncl" in

fun target =>
  let hash_release =
    target
    |> match {
      "aarch64-linux" => "8a592a0dd590e92b1c0d77631e683fc743d1ed8158e0b093b6cfabf0685089af",
      "aarch64-macos" => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
      "x86_64-linux" => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
      _ => std.fail_with "Unsupported target"
    }
  in

  let system =
    target
    |> match {
      "aarch64-linux" => "linux-aarch_64",
      "aarch64-macos" => "osx-aarch_64",
      "x86_64-linux" => "linux-x86_64",
      _ => std.fail_with "Unsupported target"
    }
  in

  let version = "25.4" in

  {
    name = "protoc",
    sandbox = build_sandbox target,
    script = m%"
        cp -r ./release/bin $output/bin
        cp -r ./release/include $output/include
        chmod +x $output/bin/protoc
      "%,
    source = {
      release = {
        hash = hash_release,
        uri = "https://github.com/protocolbuffers/protobuf/releases/download/v%{version}/protoc-%{version}-%{system}.zip"
      }
    },
    systems = ["aaarch64-linux", "aarch64-macos", "x86_64-linux"],
  } | Package

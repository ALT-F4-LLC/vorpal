let { IsLinux, .. } = import "@vorpal/cross-platform.ncl" in
let { Package, .. } = import "@vorpal/contract.ncl" in

fun config =>
  let package = config | Package in
  let package_envkey = std.string.replace "-" "_" package.name in

  let package_script = m%"
      %{package.script}

      find $output -type f | while read -r file; do
        if file "$file" | grep -q 'text'; then
          sed -i '' "s|$output|$%{package_envkey}|g" "$file"
          sed -i '' "s|$PWD|$%{package_envkey}|g" "$file"
        fi
      done

      if [ "$(uname -s)" = "Linux" ]; then
        find "$output" -type f -executable | while read -r file; do
          patchelf --set-interpreter "$glibc" "$file"
        done
      fi
    "%
  in

  package
  & {
    script = package_script |> std.string.trim,
  }

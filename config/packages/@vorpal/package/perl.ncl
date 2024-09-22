let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  let script_build_cpus =
    target
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      "x86_64-linux" => "nproc",
      _ => std.fail_with "Unsupported target"
    }
  in

  {
    name = "perl",
    script = m%"
      pushd "${PWD}/perl"
      ./Configure -des -Dprefix="$output"
      make -j$(%{script_build_cpus})
      make install

      mv "$output/bin/perl" "$output/bin/.perl-wrapped"
      wrapper="$output/bin/perl"
      touch "$wrapper"
      echo "#!/bin/sh -e" > "$wrapper"
      echo "export PERL5LIB=$output/lib/perl5/5.40.0" >> $wrapper
      echo exec $output/bin/.perl-wrapped '"$@"' >> "$wrapper"
      chmod +x "$wrapper"
    "%,
    source = {
      perl = {
        hash = "59b6437a3da1d9de0126135b31f1f16aee9c3b7a0f61f6364b2da3e8bb5f771f",
        strip_prefix = true,
        uri = "https://www.cpan.org/src/5.0/perl-5.40.0.tar.gz",
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package

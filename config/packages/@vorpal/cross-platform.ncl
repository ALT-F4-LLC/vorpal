{
  MakeCpus = fun target =>
    target
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      "x86_64-linux" => "nproc",
      "x86_64-macos" => "sysctl -n hw.ncpu",
      _ => std.fail_with "Unsupported target"
    },
}

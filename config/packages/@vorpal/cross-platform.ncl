{
  MakeCpus = fun target =>
    target
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      "x86_64-linux" => "nproc",
      _ => std.fail_with "Unsupported target"
    },
}

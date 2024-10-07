{
  IsLinux = fun system => std.string.contains "linux" system,

  IsMacos = fun system => std.string.contains "macos" system,

  MakeCpus = fun system =>
    system
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      "x86_64-linux" => "nproc",
      "x86_64-macos" => "sysctl -n hw.ncpu",
      _ => std.fail_with "Unsupported target"
    },
}

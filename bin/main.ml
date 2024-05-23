open Vorpal.Build

let example : artifact =
  {
    ignore = [ ".git"; ".gitignore"; ".direnv"; "_build" ];
    name = "example";
    source = ".";
  }

let () = build_artifact example

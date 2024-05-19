open Store

type artifact = { name : string; source : string }

let store = "/tmp/vorpal/store"
let example : artifact = { name = "example"; source = "." }
let ignore_files = [ ".git"; ".gitignore"; ".direnv"; "_build" ]

let () =
  let artifact_dir = create_dir store example.name in
  let artifact_files = copy_dir example.source artifact_dir ignore_files in
  List.iter (fun f -> Printf.printf "Copied %s\n" f) artifact_files

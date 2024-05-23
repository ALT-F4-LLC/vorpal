open Store

type artifact = { ignore : string list; name : string; source : string }

let build_artifact artifact =
  let store_dir = create_dir artifact.name in
  let store_files = copy_files artifact.source store_dir artifact.ignore in
  List.iter (fun f -> Printf.printf "Copied %s\n" f) store_files

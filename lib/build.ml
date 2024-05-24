open Hash
open Store

type artifact = { ignore : string list; name : string; source : string }

let build_artifact (artifact : artifact) =
  get_file_paths artifact.source artifact.ignore
  |> generate_hashes |> combine_hashes
  |> fun hash ->
  create_dir artifact.name hash |> fun dir ->
  copy_files artifact.source dir artifact.ignore |> List.iter print_endline

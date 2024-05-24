open Hash
open Store

type artifact = { ignore : string list; name : string; source : string }

let build_artifact (artifact : artifact) =
  let artifact_dir = create_dir artifact.name in
  copy_files artifact.source artifact_dir artifact.ignore
  |> generate_hashes
  |> List.iter (fun f -> Printf.printf "%s\n" f)

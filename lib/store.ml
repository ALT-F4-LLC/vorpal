open Common
open Syntax.Let

type t = { artifact_cache : Artifact.t File_path_map.t; path : Fpath.t }

let default_path = Fpath.v "/tmp/vorpal/store"

let make ?(path = default_path) () =
  let artifact_cache = File_path_map.empty in
  { artifact_cache; path }
;;

let artifact_path store artifact =
  Fpath.append store.path (Artifact.path artifact)
;;

let add_artifact_to_cache store artifact =
  let cache_key = artifact_path store artifact in
  let updated_cache =
    File_path_map.add cache_key artifact store.artifact_cache
  in
  Ok { store with artifact_cache = updated_cache }
;;

let build store definition =
  let@ artifact = Artifact_builder.build definition in
  let path = artifact_path store artifact in
  let@ () =
    match File_system.create_dir path with
    | Ok true -> Ok ()
    | Ok false -> Error (`Create_store_directory_error path)
    | Error cause -> Error cause
  in
  let _ =
    File_system.copy_files
      ~src:(Fpath.to_string artifact.definition.source)
      ~dst:(Fpath.to_string path)
      ~ignore:
        (Option.value ~default:[] artifact.definition.ignore
        |> List.map Fpath.to_string)
  in
  let updated_store = add_artifact_to_cache store artifact in
  Ok (updated_store, artifact)
;;

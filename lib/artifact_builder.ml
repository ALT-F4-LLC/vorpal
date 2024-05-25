open Common
open Extended
open Syntax.Let

let filter_ignored_and_dirs ignored_paths path =
  let result =
    if File_system.is_directory path then true
    else if List.mem path ignored_paths then true
    else false
  in
  not result
;;

let hash (artifact_definition : Artifact_definition.t) : (string, 'e) result =
  let ignored_paths = Option.value ~default:[] artifact_definition.ignore in
  File_system.of_path artifact_definition.source
  |> Result.map File_system.collect_file_paths
  |> Result.map (List.filter (filter_ignored_and_dirs ignored_paths))
  |> Result.map (List.map (File_system.read_file_map ~fn:String.to_sha256_hex))
  |> Result.and_then Result.sequence
  |> Result.map (List.fold_left String.join "")
  |> Result.map String.to_sha256_hex
;;

let build (definition : Artifact_definition.t) : (Artifact.t, 'e) result =
  let@ artifact_hash = hash definition in
  Ok (Artifact.make ~definition ~hash:artifact_hash)
;;

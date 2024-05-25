type t = File of Fpath.t | Directory of { path : Fpath.t; contents : t list }

let ( let@ ) = Result.bind

let collect_file_paths fs =
  let rec loop paths fs =
    match fs with
    | File path -> path :: paths
    | Directory { contents; _ } ->
        List.fold_left
          (fun accumulated_paths fs' -> loop accumulated_paths fs')
          paths contents
  in
  loop [] fs
;;

let is_directory path =
  Bos.OS.Dir.exists path
  |> Result.value ~default:false
  |> fun result -> result || Fpath.is_dir_path path
;;

let is_file path =
  Bos.OS.File.exists path
  |> Result.value ~default:false
  |> fun result -> result || Fpath.is_file_path path
;;

let list_directory path =
  match Bos.OS.Dir.contents ~dotfiles:true path with
  | Ok contents -> Ok contents
  | Error (`Msg err) -> Error (`List_directory_error err)
;;

let rec read_directory path =
  match list_directory path with
  | Ok files ->
      let rec process_files acc = function
        | [] -> Ok (Directory { path; contents = List.rev acc })
        | entry :: rest -> (
            match of_path entry with
            | Ok e -> process_files (e :: acc) rest
            | Error _ as err -> err)
      in
      process_files [] files
  | Error _ as error -> error

and of_path path =
  if is_directory path then read_directory path
  else if is_file path then Ok (File path)
  else Error (`Invalid_path path)
;;

let read_file path =
  match Bos.OS.File.read path with
  | Ok file_contents -> Ok file_contents
  | Error (`Msg msg) -> Error (`Read_file_error msg)
;;

let read_file_map ~fn path =
  let file_contents = read_file path in
  Result.map fn file_contents
;;

let create_dir path =
  match Bos.OS.Dir.create ~path:true path with
  | Ok created -> Ok created
  | Error (`Msg msg) -> Error (`Create_directory_error msg)
;;

(*TODO: Convert to using Bos*)
let copy_file (src : string) (dst : string) : unit =
  let ic = open_in src in
  let oc = open_out dst in
  try
    while true do
      output_char oc (input_char ic)
    done
  with End_of_file ->
    close_in ic;
    close_out oc
;;

(*TODO: Convert to using Bos*)
let rec copy_files ~(src : string) ~(dst : string) ~(ignore : string list) :
    string list =
  if Sys.is_directory src then (
    if not (Sys.file_exists dst) then Unix.mkdir dst 0o777;
    Sys.readdir src
    |> Array.fold_left
         (fun acc file ->
           if not (List.mem file ignore) then
             let src_file = Filename.concat src file in
             let dst_file = Filename.concat dst file in
             let copied_files =
               copy_files ~src:src_file ~dst:dst_file ~ignore
             in
             acc @ copied_files
           else acc)
         [])
  else if not (List.mem (Filename.basename src) ignore) then (
    copy_file src dst;
    [ dst ])
  else []
;;

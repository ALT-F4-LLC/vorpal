let store = "/tmp/vorpal/store"

let rec get_file_paths (path : string) (ignore_files : string list) =
  if Sys.is_directory path then
    Sys.readdir path
    |> Array.fold_left
         (fun acc file ->
           if not (List.mem file ignore_files) then
             let file_path = Unix.realpath (Filename.concat path file) in
             let files = get_file_paths file_path ignore_files in
             acc @ files
           else acc)
         []
  else if not (List.mem (Filename.basename path) ignore_files) then [ path ]
  else []

let get_dir_path (name : string) (hash : string) : string =
  Filename.concat store (name ^ "-" ^ hash)

let dir_exists (name : string) (hash : string) =
  try
    let stats = Unix.stat (get_dir_path name hash) in
    stats.st_kind = S_DIR
  with
  | Unix.Unix_error (ENOENT, _, _) -> false (* No such file or directory *)
  | Unix.Unix_error (EACCES, _, _) -> false (* Permission denied *)
  | _ -> false (* Any other error *)

let create_dir (name : string) (hash : string) : string =
  let dir_path = get_dir_path name hash in
  Unix.mkdir dir_path 0o777;
  dir_path

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

let rec copy_files (src : string) (dst : string) (ignore_files : string list) :
    string list =
  if Sys.is_directory src then (
    if not (Sys.file_exists dst) then Unix.mkdir dst 0o777;
    Sys.readdir src
    |> Array.fold_left
         (fun acc file ->
           if not (List.mem file ignore_files) then
             let src_file = Filename.concat src file in
             let dst_file = Filename.concat dst file in
             let copied_files = copy_files src_file dst_file ignore_files in
             acc @ copied_files
           else acc)
         [])
  else if not (List.mem (Filename.basename src) ignore_files) then (
    copy_file src dst;
    [ dst ])
  else []

let read_file (file : string) : string =
  let ic = open_in file in
  let n = in_channel_length ic in
  let s = really_input_string ic n in
  close_in ic;
  s

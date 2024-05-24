let store = "/tmp/vorpal/store"

let create_dir (artifact_name : string) : string =
  let dir_path = Filename.concat store artifact_name in
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

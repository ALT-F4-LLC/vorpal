module List = struct
  include List

  let chunk (chunk_size : int) (list : 'a list) =
    let rec aux (acc : 'a list list) (chunk : 'a list)
        (current_chunk_size : int) (list' : 'a list) =
      match list' with
      | [] -> if chunk = [] then acc else List.rev chunk :: acc
      | hd :: tl ->
          if current_chunk_size < chunk_size then
            aux acc (hd :: chunk) (current_chunk_size + 1) tl
          else aux (List.rev chunk :: acc) [ hd ] 1 tl
    in
    List.rev (aux [] [] 0 list)
  ;;

  let hd_opt (list : 'a list) : 'a option =
    match list with [] -> None | hd :: _ -> Some hd
  ;;
end

module String = struct
  include String

  let join s1 s2 = s1 ^ s2

  let to_sha256_hex str =
    let open Digestif in
    str |> SHA256.of_raw_string |> SHA256.to_hex
  ;;
end

module Result = struct
  include Result

  let sequence result_list =
    result_list
    |> List.fold_left
         (fun acc result ->
           match result with
           | Error _ as error -> error
           | Ok value -> acc |> Result.map (fun list' -> value :: list'))
         (Ok [])
    |> Result.map List.rev
  ;;

  let and_then fn result = Result.bind result fn
end

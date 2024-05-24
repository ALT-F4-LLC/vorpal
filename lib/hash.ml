open Mirage_crypto.Hash
open Store

let generate_hash (input : string) : string =
  let digest = SHA256.digest (Cstruct.of_string input) in
  Cstruct.to_hex_string digest

let generate_hashes (files : string list) : string list =
  List.map
    (fun f ->
      let data = read_file f in
      let hash = generate_hash data in
      hash)
    files

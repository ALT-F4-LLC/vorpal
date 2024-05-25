open Vorpal
open Common.Syntax.Let

let main () =
  let store = Store.make () in
  let ignore =
    [ ".git"; ".gitignore"; ".direnv"; "_build" ] |> List.map Fpath.v
  in
  let definition =
    Artifact_definition.make ~ignore ~name:"example" ~source:(Fpath.v ".") ()
  in
  let@ updated_store, artifact = Store.build store definition in
  Ok (updated_store, artifact)
;;

let _ = main ()

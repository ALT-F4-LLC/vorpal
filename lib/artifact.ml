type t = { definition : Artifact_definition.t; hash : string }

let make ~definition ~hash = { definition; hash }
let definition artifact = artifact.definition
let name artifact = artifact.definition.name
let hash artifact = artifact.hash
let path artifact = name artifact ^ "-" ^ hash artifact |> Fpath.v

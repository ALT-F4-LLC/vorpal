type t = { ignore : Fpath.t list option; name : string; source : Fpath.t }
[@@deriving eq, ord, show]

let make ?ignore ~name ~source () = { name; ignore; source }
let name t = t.name
let source t = t.source
let ignore t = t.ignore

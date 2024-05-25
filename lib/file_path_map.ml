include Map.Make (struct
  type t = Fpath.t

  let compare = compare
end)

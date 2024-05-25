module Infix = struct
  module Option = struct
    (** [>>=] is an infix [Option.bind]. *)
    let ( >>= ) = Option.bind

    (** [>|=] is an infix left-to-right [Option.map]. *)
    let ( >|= ) opt_a f = Option.map f opt_a
  end

  module Result = struct
    (** [>>=] is an infix [Result.bind]. *)
    let ( >>= ) = Result.bind

    (** [>|=] is an infix left-to-right [Result.map]. *)
    let ( >|= ) res_a f = Result.map f res_a

    (** 
      * [>|?] is an infix operator for passing [Ok] values through 
      * or applying [f] to [Error] values. 
      *)
    let ( >|? ) v f = Result.map_error f v
  end
end

module Let = struct
  (** [let- var = opt] binds [var] to [v] when [opt] is [Some v] *)
  let ( let- ) = Option.bind

  (** [let@ var = res] binds [var] to [v] when [res] is [Ok v] *)
  let ( let@ ) = Result.bind
end

(* Signal definitions. *)

open Types

let make id name color note = { id; name; color; note }

let summary s =
  Printf.sprintf "#%d %s (%s)" s.id s.name (Types.string_of_color s.color)

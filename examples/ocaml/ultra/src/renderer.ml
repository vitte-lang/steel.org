(* Render status lines. *)

open Types

let render_signal s =
  Printf.sprintf "%-6s | %-8s | %s" (Types.string_of_color s.color) s.name s.note

let render_reading r =
  Printf.sprintf "t=%.2f id=%d v=%.2f" r.ts r.signal_id r.value

(* Basic stats on readings. *)

open Types

let avg readings =
  match readings with
  | [] -> 0.0
  | _ ->
      let sum = List.fold_left (fun acc r -> acc +. r.value) 0.0 readings in
      sum /. Float.of_int (List.length readings)

let max_reading readings =
  List.fold_left (fun acc r -> if r.value > acc then r.value else acc) 0.0 readings

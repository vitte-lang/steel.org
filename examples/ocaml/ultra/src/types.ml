(* Domain types for "Signal Garden" example. *)

type color =
  | Red
  | Amber
  | Green

let string_of_color = function
  | Red -> "red"
  | Amber -> "amber"
  | Green -> "green"

type signal_id = int

type signal = {
  id : signal_id;
  name : string;
  color : color;
  note : string;
}

type reading = {
  ts : float;
  signal_id : signal_id;
  value : float;
}

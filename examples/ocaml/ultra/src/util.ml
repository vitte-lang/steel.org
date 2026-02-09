(* Small utilities. *)

let clamp ~min_v ~max_v v =
  if v < min_v then min_v else if v > max_v then max_v else v

let round2 v =
  Float.of_int (int_of_float (v *. 100.0)) /. 100.0

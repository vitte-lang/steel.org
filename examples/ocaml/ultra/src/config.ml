(* Configuration defaults. *)

type t = {
  max_signals : int;
  sample_interval_s : float;
}

let default = {
  max_signals = 12;
  sample_interval_s = 0.5;
}

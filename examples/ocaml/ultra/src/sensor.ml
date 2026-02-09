(* Fake sensor producing readings. *)

open Types
open Util

let read ~signal_id ~seed =
  let base = Float.of_int ((signal_id * 37 + seed) mod 100) /. 10.0 in
  let jitter = Float.of_int (seed mod 7) /. 10.0 in
  let v = base +. jitter in
  { ts = Clock.now (); signal_id; value = round2 (clamp ~min_v:0.0 ~max_v:10.0 v) }

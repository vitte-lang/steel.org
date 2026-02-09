(* Build a sample signal garden. *)

open Types

let seed_signals () =
  [
    Signal.make 1 "Rose" Red "Heat spike";
    Signal.make 2 "Lily" Amber "Moisture drop";
    Signal.make 3 "Oak" Green "Steady growth";
    Signal.make 4 "Fern" Green "Low light";
    Signal.make 5 "Iris" Amber "Mineral shift";
  ]

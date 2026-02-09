(* In-memory store for signals + readings. *)

open Types

type t = {
  signals : (signal_id, signal) Hashtbl.t;
  mutable readings : reading list;
}

let create () = {
  signals = Hashtbl.create 32;
  readings = [];
}

let add_signal t s = Hashtbl.replace t.signals s.id s

let add_reading t r = t.readings <- r :: t.readings

let list_signals t =
  Hashtbl.to_seq_values t.signals |> List.of_seq

let latest_readings t ~limit =
  t.readings |> List.rev |> List.filteri (fun i _ -> i < limit)

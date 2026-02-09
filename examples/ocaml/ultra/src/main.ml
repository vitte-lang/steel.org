open Types

let () =
  let store = Store.create () in
  Garden.seed_signals () |> List.iter (Store.add_signal store);

  Printf.printf "%s\n" (Cli.banner ());
  Store.list_signals store
  |> List.iter (fun s -> Printf.printf "%s\n" (Renderer.render_signal s));

  (* Generate a few readings. *)
  let seed = int_of_float (Unix.gettimeofday ()) in
  Store.list_signals store
  |> List.iter (fun s ->
      let r = Sensor.read ~signal_id:s.id ~seed in
      Store.add_reading store r);

  let readings = Store.latest_readings store ~limit:5 in
  List.iter (fun r -> Printf.printf "%s\n" (Renderer.render_reading r)) readings;
  Printf.printf "avg=%.2f max=%.2f\n" (Stats.avg readings) (Stats.max_reading readings)

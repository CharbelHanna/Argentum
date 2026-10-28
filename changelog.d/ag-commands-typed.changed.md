- **The `ag` commands are typed end to end.** `mods/dispatch.rs` lists every JSON command once,
  in a `commands!` table: its name, its argument struct, its answer type and its body. The table
  makes the match that routes each call, and a test (`mods/ag_contract.rs`) writes
  `src/argentum/ag.generated.ts` from the same table with ts-rs, as the engine's contract is
  written, so `ag('name', args)` in the interface is checked by name, argument and answer. A
  command renamed, an argument changed or an answer read as the wrong type no longer compiles.
  The 40 TypeScript types that restated Rust structs by hand are gone, along with 58 explicit
  `ag<T>` type arguments and the per-command JSON conversion in `dispatch.rs`. ts-rs is a
  development dependency only; nothing shipped compiles it, and no command's behaviour changed.

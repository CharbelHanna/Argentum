- **An `ag` command calls its module directly.** 28 functions in `mods/commands.rs` only passed
  their arguments on to another module (`highlights::save`, `editor_layout::theme`,
  `profiles::cameras`, ...); each `commands!` table line in `mods/dispatch.rs` now makes that call
  itself, and the forwarders are gone. `commands.rs` keeps the 14 commands with logic of their
  own. The profile library's "not ready yet" check moved beside the library, as
  `profiles::library_ready`. No command's behaviour changed.

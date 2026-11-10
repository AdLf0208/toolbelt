// End of a running server tick: open queued belt menus and refresh the open
// ones.
fn observe_server_tick(game: &mut crate::game::Game<'_>, phase: ServerTickPhase) {
    crate::game::toolbelt_state::on_server_tick(game, phase);
}

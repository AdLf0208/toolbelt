// Runs the `/toolbelt` command fallback (the acting player's state is swapped
// in here, so the same inventory path as the use hooks is used).
fn run_toolbelt_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    crate::game::toolbelt_state::on_perform_command(game, player, command, ran);
}

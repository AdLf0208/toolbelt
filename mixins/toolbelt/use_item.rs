// A right click in the air with the belt: insert the off hand's stack, sneak
// to extract, else queue the belt menu.
fn observe_use_item(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    claimed: &mut bool,
) {
    crate::game::toolbelt_state::on_use_item(game, stack, claimed);
}

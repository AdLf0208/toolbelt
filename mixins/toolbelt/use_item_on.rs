// A right click on a block with the belt: the same as the air click, and it
// claims the click so the block is not also used.
fn observe_use_item_on(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    pos: [i32; 3],
    face: [i32; 3],
    claimed: &mut bool,
) {
    crate::game::toolbelt_state::on_use_item_on(game, stack, pos, face, claimed);
}

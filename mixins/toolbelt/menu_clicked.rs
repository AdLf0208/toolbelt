// A click in the belt menu: a belt slot's stack goes to the player.
fn observe_menu_clicked(
    game: &mut crate::game::Game<'_>,
    uuid: u128,
    window: i32,
    slot: i32,
) {
    crate::game::toolbelt_state::on_menu_clicked(game, uuid, window, slot);
}

// The belt menu closed (or another menu replaced it): forget its state.
fn observe_menu_closed(game: &mut crate::game::Game<'_>, uuid: u128, window: i32) {
    crate::game::toolbelt_state::on_menu_closed(game, uuid, window);
}

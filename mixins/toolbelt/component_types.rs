// Runs before content loads: registers the belt's custom component ids.
fn register_component_types(types: &mut Vec<String>) {
    crate::game::toolbelt_state::register_component_types(types);
}

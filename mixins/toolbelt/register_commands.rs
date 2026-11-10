// Declares `/toolbelt put|take|info|menu|help` at permission level 2.
fn collect_toolbelt_commands(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    crate::game::toolbelt_state::on_register_commands(commands);
}

// Game-side glue for the Tool Belt slice: the belt's item components, the
// deferred belt menu, the right-click insert/extract, and the `/toolbelt`
// command fallback.
//
// Injected into `crate::game` as `toolbelt_state`; it needs the game, so it is
// not part of the standalone library (only `runtime.rs` is).
//
// The menu cannot be opened inside a use hook (the hub is held by the guest
// turn), so a plain use queues it and `server_tick` End opens it with
// `native_open_menu`. Clicks arrive in `native_menu_clicked`, where no
// player's state is swapped in: the guest record in the hub is read and
// written directly.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use horizon_client::first_person::Hand;
use horizon_client::item_data::{decode_components, encode_components};
use horizon_client::items::ItemStack;
use horizon_client::text::Text;
use horizon_world::nbt::{Compound, JavaString, Tag};

use crate::game::native_server::{NativeCommand, NativeServerPlayer};
use crate::game::toolbelt::{
    BELT_ITEM, BeltStack, DEFAULT_SIZE, Insertion, Policy, allows, clamp_size, find_insertion,
    slot_count,
};

/// The two custom component ids, registered before content loads.
const BELT_SIZE: &str = "toolbelt:belt_size";
const BELT_CONTENTS: &str = "toolbelt:belt_contents";

/// The menu title and the most rows a belt menu needs (one row holds all nine
/// belt slots).
const TITLE: &str = "Tool Belt";
const MENU_ROWS: u8 = 1;

#[derive(Default)]
struct State {
    /// A belt use waiting for the server tick to open its menu, by player.
    pending: BTreeMap<u128, usize>,
    /// The belt menu a player has open: its window, inventory slot and size.
    open: BTreeMap<u128, OpenBelt>,
    /// Ticks left before another right-click is handled for a player: one
    /// physical click arrives as use-on-block + air use.
    guard: BTreeMap<u128, u8>,
}

#[derive(Clone)]
struct OpenBelt {
    window: i32,
    slot: usize,
    size: u8,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Registers the belt's two component ids. Runs before content loads, so the
/// carried item may name them as defaults.
pub(crate) fn register_component_types(types: &mut Vec<String>) {
    for id in [BELT_SIZE, BELT_CONTENTS] {
        types.push(id.to_owned());
    }
}

// ---------------------------------------------------------------- components

fn is_belt(stack: &ItemStack) -> bool {
    &*stack.item == BELT_ITEM
}

fn max_stack(game: &crate::game::Game<'_>, item: &str) -> u32 {
    game.res
        .items()
        .get(item)
        .map_or(64, |info| info.max_stack.max(1))
}

fn belt_size(game: &crate::game::Game<'_>, belt: &ItemStack) -> u8 {
    match game.native_component(belt, BELT_SIZE) {
        Ok(Some(Tag::Int(value))) => clamp_size(u8::try_from(*value).unwrap_or(DEFAULT_SIZE)),
        _ => DEFAULT_SIZE,
    }
}

/// A descriptor for the pure engine: item, count, the item's max stack, and a
/// fingerprint of the components (`ItemStack.isSameItemSameComponents`).
fn describe(game: &crate::game::Game<'_>, stack: &ItemStack) -> BeltStack {
    BeltStack {
        item: stack.item.to_string(),
        count: stack.count,
        max_stack: max_stack(game, &stack.item),
        variant: variant_of(stack),
    }
}

fn variant_of(stack: &ItemStack) -> u64 {
    match encode_components(stack) {
        Some(tag) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            format!("{tag:?}").hash(&mut hasher);
            hasher.finish()
        }
        None => 0,
    }
}

fn allowed(game: &crate::game::Game<'_>, stack: &ItemStack) -> bool {
    allows(&Policy::default(), &describe(game, stack))
}

fn read_slots(
    game: &crate::game::Game<'_>,
    belt: &ItemStack,
    size: u8,
) -> Vec<Option<ItemStack>> {
    let mut slots = vec![None; slot_count(size)];
    let Ok(Some(Tag::List(entries))) = game.native_component(belt, BELT_CONTENTS) else {
        return slots;
    };
    for entry in entries {
        let Some((index, stack)) = entry_from_tag(entry) else {
            continue;
        };
        if let Some(slot) = slots.get_mut(index) {
            *slot = Some(stack);
        }
    }
    slots
}

fn write_belt(
    game: &crate::game::Game<'_>,
    belt: &mut ItemStack,
    size: u8,
    slots: &[Option<ItemStack>],
) {
    let _ = game.native_set_component(belt, BELT_SIZE, Tag::Int(i32::from(clamp_size(size))));
    let entries: Vec<Tag> = slots
        .iter()
        .enumerate()
        .filter_map(|(index, stack)| stack.as_ref().map(|stack| entry_tag(index, stack)))
        .collect();
    let _ = game.native_set_component(belt, BELT_CONTENTS, Tag::List(entries));
}

/// One stored stack: `{slot, id, count, components}`.
fn entry_tag(slot: usize, stack: &ItemStack) -> Tag {
    let components = match encode_components(stack) {
        Some(Tag::Compound(compound)) => compound,
        _ => Compound::new(),
    };
    let mut entry = Compound::new();
    entry.insert(
        JavaString::from("slot"),
        Tag::Int(i32::try_from(slot).unwrap_or(i32::MAX)),
    );
    entry.insert(
        JavaString::from("id"),
        Tag::String(JavaString::from(stack.item.as_ref())),
    );
    entry.insert(
        JavaString::from("count"),
        Tag::Int(i32::try_from(stack.count.max(1)).unwrap_or(i32::MAX)),
    );
    entry.insert(JavaString::from("components"), Tag::Compound(components));
    Tag::Compound(entry)
}

fn entry_from_tag(tag: &Tag) -> Option<(usize, ItemStack)> {
    let Tag::Compound(entry) = tag else {
        return None;
    };
    let slot = match entry.get(&JavaString::from("slot"))? {
        Tag::Int(slot) => usize::try_from(*slot).ok()?,
        _ => return None,
    };
    let id = match entry.get(&JavaString::from("id"))? {
        Tag::String(value) => value.to_string_lossy(),
        _ => return None,
    };
    let count = match entry.get(&JavaString::from("count")) {
        Some(Tag::Int(count)) => u32::try_from((*count).max(1)).unwrap_or(1),
        _ => 1,
    };
    let (damage, extra) = decode_components(entry.get(&JavaString::from("components")));
    Some((
        slot,
        ItemStack {
            item: std::sync::Arc::from(id),
            count,
            damage,
            extra,
        },
    ))
}

// ---------------------------------------------------------------- storage ops

/// Inserts as much of `incoming` as fits; returns what did not fit.
fn insert_into(
    game: &crate::game::Game<'_>,
    belt: &mut ItemStack,
    mut incoming: ItemStack,
) -> Option<ItemStack> {
    // `ConfigData.isItemStackAllowed`: nothing that the belt refuses ever lands,
    // whatever the path (command, click, a future hopper).
    if incoming.count == 0 || !allowed(game, &incoming) {
        return (incoming.count > 0).then_some(incoming);
    }
    let size = belt_size(game, belt);
    let mut slots = read_slots(game, belt, size);
    // Up to nine slots and one item per placement: a generous loop bound.
    for _ in 0..64 {
        if incoming.count == 0 {
            break;
        }
        let descriptions: Vec<Option<BeltStack>> = slots
            .iter()
            .map(|slot| slot.as_ref().map(|stack| describe(game, stack)))
            .collect();
        match find_insertion(&descriptions, size, &describe(game, &incoming)) {
            Insertion::Merge { slot, room } => {
                let put = room.min(incoming.count);
                if let Some(Some(existing)) = slots.get_mut(slot) {
                    existing.count += put;
                }
                incoming.count -= put;
            }
            Insertion::Empty { slot } => {
                let cap = max_stack(game, &incoming.item);
                let put = incoming.count.min(cap);
                if let Some(target) = slots.get_mut(slot) {
                    *target = Some(ItemStack {
                        count: put,
                        ..incoming.clone()
                    });
                }
                incoming.count -= put;
            }
            Insertion::Full => break,
        }
    }
    write_belt(game, belt, size, &slots);
    (incoming.count > 0).then_some(incoming)
}

/// Takes up to `count` from belt slot `slot`; `None` when it is empty.
fn extract_from(
    game: &crate::game::Game<'_>,
    belt: &mut ItemStack,
    slot: usize,
    count: u32,
) -> Option<ItemStack> {
    if count == 0 {
        return None;
    }
    let size = belt_size(game, belt);
    let mut slots = read_slots(game, belt, size);
    let existing = slots.get_mut(slot)?.as_mut()?;
    let taken = count.min(existing.count);
    if taken == 0 {
        return None;
    }
    let out = ItemStack {
        count: taken,
        ..existing.clone()
    };
    existing.count -= taken;
    if existing.count == 0 {
        slots[slot] = None;
    }
    write_belt(game, belt, size, &slots);
    Some(out)
}

/// The first occupied slot, for a sneak extract.
fn first_occupied(
    game: &crate::game::Game<'_>,
    belt: &ItemStack,
) -> Option<usize> {
    read_slots(game, belt, belt_size(game, belt))
        .iter()
        .position(Option::is_some)
}

/// Gives `input` to the acting player's inventory (inside a use/command hook),
/// merging then filling empty slots; returns how many were placed.
fn give_to_acting(game: &mut crate::game::Game<'_>, input: ItemStack) -> u32 {
    let max = max_stack(game, &input.item);
    let mut remaining = input.count;
    for slot in game
        .inventory
        .iter_mut()
        .take(36)
        .chain(std::iter::once(&mut game.offhand))
    {
        if remaining == 0 {
            break;
        }
        if let Some(existing) = slot
            && existing.same_item_same_components(&input)
            && existing.count < max
        {
            let put = (max - existing.count).min(remaining);
            existing.count += put;
            remaining -= put;
        }
    }
    for slot in game
        .inventory
        .iter_mut()
        .take(36)
        .chain(std::iter::once(&mut game.offhand))
    {
        if remaining == 0 {
            break;
        }
        if slot.is_none() {
            let put = remaining.min(max);
            *slot = Some(ItemStack {
                count: put,
                ..input.clone()
            });
            remaining -= put;
        }
    }
    input.count - remaining
}

// ------------------------------------------------------------------- use hooks

/// A right click in the air with the belt: insert the off hand's stack, sneak
/// to extract, else queue the menu.
#[allow(private_interfaces)] // the injected module names the game's private Game type.
pub(crate) fn on_use_item(
    game: &mut crate::game::Game<'_>,
    stack: &ItemStack,
    claimed: &mut bool,
) {
    handle_click(game, stack, claimed);
}

/// A right click on a block with the belt: the same, and claim the click so
/// the block is not also used.
#[allow(private_interfaces)] // the injected module names the game's private Game type.
pub(crate) fn on_use_item_on(
    game: &mut crate::game::Game<'_>,
    stack: &ItemStack,
    _pos: [i32; 3],
    _face: [i32; 3],
    claimed: &mut bool,
) {
    handle_click(game, stack, claimed);
}

fn handle_click(
    game: &mut crate::game::Game<'_>,
    stack: &ItemStack,
    claimed: &mut bool,
) {
    if !matches!(game.hand, Hand::Main) || !is_belt(stack) {
        return;
    }
    let stack = stack.clone();
    let Some(player) = game.native_acting_player() else {
        return;
    };
    let guarded = {
        let guard = state();
        guard
            .as_ref()
            .and_then(|state| state.guard.get(&player.uuid))
            .is_some_and(|ticks| *ticks > 0)
    };
    if guarded {
        return;
    }
    *claimed = true;
    let slot = game.held_slot();
    let mut handled = true;
    if sneaking(game) {
        extract_into_inventory(game, &stack, None);
    } else if let Some(offhand) = game.offhand.clone().filter(|s| allowed(game, s)) {
        let mut belt = stack;
        let leftover = insert_into(game, &mut belt, offhand);
        game.offhand = leftover;
        *game.held_mut() = Some(belt);
    } else {
        handled = false;
        let mut guard = state();
        let state = guard.get_or_insert_with(State::default);
        state.pending.insert(player.uuid, slot);
    }
    if handled {
        *claimed = true;
    }
    let mut guard = state();
    let state = guard.get_or_insert_with(State::default);
    state.guard.insert(player.uuid, 2);
}

/// Whether the acting player is sneaking (`PlayerMotion.shift_key_down`).
fn sneaking(game: &crate::game::Game<'_>) -> bool {
    game.player
        .as_ref()
        .is_some_and(|player| player.shift_key_down)
}

/// Extracts one belt stack into the acting player's inventory.
fn extract_into_inventory(
    game: &mut crate::game::Game<'_>,
    belt: &ItemStack,
    slot: Option<usize>,
) {
    let Some(index) = slot.or_else(|| first_occupied(game, belt)) else {
        return;
    };
    let mut belt = belt.clone();
    let Some(taken) = extract_from(game, &mut belt, index, 64) else {
        return;
    };
    give_to_acting(game, taken);
    *game.held_mut() = Some(belt);
}

// ------------------------------------------------------------- menu lifecycle

/// At the end of a server tick: open queued belt menus and refresh the open
/// ones from the belt item.
#[allow(private_interfaces)] // the injected module names the game's private Game type.
pub(crate) fn on_server_tick(
    game: &mut crate::game::Game<'_>,
    phase: crate::game::native_lifecycle::ServerTickPhase,
) {
    let crate::game::native_lifecycle::ServerTickPhase::ServerTickEnd { world_running } = phase
    else {
        return;
    };
    if !world_running {
        return;
    }
    let (pending, open, _) = {
        let mut guard = state();
        let state = guard.get_or_insert_with(State::default);
        for ticks in state.guard.values_mut() {
            *ticks = ticks.saturating_sub(1);
        }
        (
            std::mem::take(&mut state.pending),
            state.open.clone(),
            (),
        )
    };
    for (uuid, slot) in pending {
        let Some(belt) = guest_slot(game, uuid, slot) else {
            continue;
        };
        if !is_belt(&belt) {
            continue;
        }
        let size = belt_size(game, &belt);
        let slots = read_slots(game, &belt, size);
        let items = menu_items(size, &slots);
        if let Some(window) = game.native_open_menu(uuid, MENU_ROWS, Text::literal(TITLE), items)
        {
            let mut guard = state();
            let state = guard.get_or_insert_with(State::default);
            state.open.insert(
                uuid,
                OpenBelt {
                    window,
                    slot,
                    size,
                },
            );
        }
    }
    for (uuid, belt) in open {
        refresh_menu(game, uuid, &belt);
    }
}

/// Updates an open menu to the belt's current contents.
fn refresh_menu(game: &mut crate::game::Game<'_>, uuid: u128, open: &OpenBelt) {
    let Some(stack) = guest_slot(game, uuid, open.slot) else {
        return;
    };
    if !is_belt(&stack) {
        game.native_close_menu(uuid);
        let mut guard = state();
        let state = guard.get_or_insert_with(State::default);
        state.open.remove(&uuid);
        return;
    }
    let slots = read_slots(game, &stack, open.size);
    game.native_set_menu_items(uuid, menu_items(open.size, &slots));
}

/// A click in the belt menu: a belt slot's stack goes to the player.
#[allow(private_interfaces)] // the injected module names the game's private Game type.
pub(crate) fn on_menu_clicked(
    game: &mut crate::game::Game<'_>,
    uuid: u128,
    window: i32,
    slot: i32,
) {
    let Some(open) = ({ state().as_ref().and_then(|s| s.open.get(&uuid)).cloned() }) else {
        return;
    };
    if open.window != window {
        return;
    }
    let Ok(index) = usize::try_from(slot) else {
        return;
    };
    if index >= usize::from(open.size) {
        return;
    }
    let Some(belt) = guest_slot(game, uuid, open.slot) else {
        return;
    };
    if !is_belt(&belt) {
        return;
    }
    let mut belt = belt;
    let Some(taken) = extract_from(game, &mut belt, index, 64) else {
        return;
    };
    if !write_guest_slot(game, uuid, open.slot, belt) {
        return;
    }
    give_to_guest(game, uuid, taken);
    refresh_menu(game, uuid, &open);
}

/// The belt menu closed (or another menu replaced it).
#[allow(private_interfaces)] // the injected module names the game's private Game type.
pub(crate) fn on_menu_closed(game: &mut crate::game::Game<'_>, uuid: u128, window: i32) {
    let _ = game;
    let mut guard = state();
    let state = guard.get_or_insert_with(State::default);
    if state.open.get(&uuid).is_some_and(|open| open.window == window) {
        state.open.remove(&uuid);
    }
}

fn menu_items(size: u8, slots: &[Option<ItemStack>]) -> Vec<Option<ItemStack>> {
    let count = usize::from(MENU_ROWS) * 9;
    let mut items = vec![None; count];
    for (index, stack) in slots.iter().take(usize::from(size)).enumerate() {
        if index < count {
            items[index] = stack.clone();
        }
    }
    items
}

// --------------------------------------------------------------- guest access

fn guest_index(game: &crate::game::Game<'_>, uuid: u128) -> Option<usize> {
    let host = game.lan.as_ref()?;
    host.hub
        .guests
        .iter()
        .position(|guest| guest.placed && !guest.dead && guest.uuid == uuid)
}

fn guest_slot(game: &crate::game::Game<'_>, uuid: u128, slot: usize) -> Option<ItemStack> {
    let host = game.lan.as_ref()?;
    let guest = host
        .hub
        .guests
        .iter()
        .find(|guest| guest.placed && !guest.dead && guest.uuid == uuid)?;
    guest.inventory.get(slot).and_then(|stack| stack.clone())
}

fn write_guest_slot(
    game: &mut crate::game::Game<'_>,
    uuid: u128,
    slot: usize,
    stack: ItemStack,
) -> bool {
    let Some(index) = guest_index(game, uuid) else {
        return false;
    };
    let Some(host) = game.lan.as_mut() else {
        return false;
    };
    {
        let guest = &mut host.hub.guests[index];
        if guest.inventory.len() < 36 {
            guest.inventory.resize(36, None);
        }
        let Some(target) = guest.inventory.get_mut(slot) else {
            return false;
        };
        *target = Some(stack);
    }
    host.hub.send_inventory(index);
    true
}

/// Adds `stack` to a guest's inventory (`Inventory.add` order), for a menu
/// click.
fn give_to_guest(game: &mut crate::game::Game<'_>, uuid: u128, mut stack: ItemStack) {
    let Some(index) = guest_index(game, uuid) else {
        return;
    };
    let max = max_stack(game, &stack.item);
    let Some(host) = game.lan.as_mut() else {
        return;
    };
    let guest = &mut host.hub.guests[index];
    if guest.inventory.len() < 36 {
        guest.inventory.resize(36, None);
    }
    for slot in guest.inventory.iter_mut().take(36) {
        if stack.count == 0 {
            break;
        }
        if let Some(existing) = slot
            && existing.same_item_same_components(&stack)
            && existing.count < max
        {
            let put = (max - existing.count).min(stack.count);
            existing.count += put;
            stack.count -= put;
        }
    }
    for slot in guest.inventory.iter_mut().take(36) {
        if stack.count == 0 {
            break;
        }
        if slot.is_none() {
            let put = stack.count.min(max);
            *slot = Some(ItemStack {
                count: put,
                ..stack.clone()
            });
            stack.count -= put;
        }
    }
    host.hub.send_inventory(index);
}

// ------------------------------------------------------------------- commands

const ROOT: &str = "toolbelt";

const HELP: [&str; 5] = [
    "/toolbelt put <item> <count>   insert from your inventory",
    "/toolbelt take <slot> <count>  move a belt slot to your inventory",
    "/toolbelt info                 size and contents of the held belt",
    "/toolbelt menu                 open the held belt's menu",
    "/toolbelt help                 this text",
];

fn parse(command: &str) -> Action {
    let mut words = command.split_whitespace();
    match words.next() {
        Some(ROOT) => {}
        _ => return Action::Unknown,
    }
    match words.next() {
        Some("put") => {
            let item = words.next().unwrap_or_default().to_owned();
            let count = words.next().and_then(|w| w.parse().ok()).unwrap_or(1);
            Action::Put(item, count)
        }
        Some("take") => {
            let slot = words.next().and_then(|w| w.parse().ok());
            let count = words.next().and_then(|w| w.parse().ok()).unwrap_or(64);
            match slot {
                Some(slot) => Action::Take(slot, count),
                None => Action::Unknown,
            }
        }
        Some("info") => Action::Info,
        Some("menu") => Action::Menu,
        Some("help") | None => Action::Help,
        _ => Action::Unknown,
    }
}

enum Action {
    Unknown,
    Help,
    Info,
    Menu,
    Put(String, u32),
    Take(usize, u32),
}

#[allow(private_interfaces)] // the injected module names the game's private Game type.
pub(crate) fn on_register_commands(commands: &mut Vec<NativeCommand>) {
    for usage in [
        format!("{ROOT} put <item:text> <count:integer>"),
        format!("{ROOT} take <slot:integer> <count:integer>"),
        format!("{ROOT} info"),
        format!("{ROOT} menu"),
        format!("{ROOT} help"),
    ] {
        commands.push(NativeCommand::op(&usage, 2));
    }
}

#[allow(private_interfaces)] // the injected module names the game's private Game type.
pub(crate) fn on_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    match parse(command) {
        Action::Unknown => {}
        Action::Help => {
            *ran = true;
            for line in HELP {
                game.native_feedback(Text::literal(line));
            }
        }
        Action::Info => {
            *ran = true;
            let line = info_line(game);
            game.native_feedback(Text::literal(line));
        }
        Action::Menu => {
            *ran = true;
            if open_from_command(game, player.uuid) {
                game.native_feedback(Text::literal("Tool Belt menu opened."));
            } else {
                game.native_feedback(Text::literal("Hold the belt first."));
            }
        }
        Action::Put(item, count) => {
            *ran = true;
            let line = put_command(game, &item, count);
            game.native_feedback(Text::literal(line));
        }
        Action::Take(slot, count) => {
            *ran = true;
            let line = take_command(game, slot, count);
            game.native_feedback(Text::literal(line));
        }
    }
}

fn info_line(game: &crate::game::Game<'_>) -> String {
    let Some(belt) = game.held().filter(|stack| is_belt(stack)) else {
        return "Hold the belt first.".to_owned();
    };
    let size = belt_size(game, belt);
    let slots = read_slots(game, belt, size);
    let used = slots.iter().filter(|slot| slot.is_some()).count();
    let contents = slots
        .iter()
        .enumerate()
        .filter_map(|(index, stack)| {
            stack.as_ref().map(|stack| format!("{index}:{}({})", stack.item, stack.count))
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "toolbelt:belt size={size} slots={used}/{} contents=[{contents}]",
        slot_count(size)
    )
}

fn put_command(game: &mut crate::game::Game<'_>, item: &str, count: u32) -> String {
    let Some(belt) = game.held().filter(|stack| is_belt(stack)).cloned() else {
        return "Hold the belt first.".to_owned();
    };
    if count == 0 {
        return "Count must be at least 1.".to_owned();
    }
    let available = game
        .inventory
        .iter()
        .take(36)
        .chain(std::iter::once(&game.offhand))
        .flatten()
        .filter(|stack| &*stack.item == item)
        .map(|stack| stack.count)
        .sum::<u32>();
    if available == 0 {
        return format!("Carry {item} first.");
    }
    let take = count.min(available);
    let taken = take_from_acting(game, item, take);
    if taken == 0 {
        return format!("Carry {item} first.");
    }
    let incoming = ItemStack {
        item: std::sync::Arc::from(item),
        count: taken,
        damage: 0,
        extra: None,
    };
    let mut belt = belt;
    let leftover = insert_into(game, &mut belt, incoming.clone());
    let refused = leftover.map_or(0, |stack| stack.count);
    let inserted = taken - refused;
    if refused > 0 {
        // Put the refused items back before writing the belt.
        give_to_acting(
            game,
            ItemStack {
                count: refused,
                ..incoming
            },
        );
    }
    *game.held_mut() = Some(belt);
    if inserted == 0 {
        format!("The belt does not take {item}.")
    } else {
        format!("Inserted {inserted} {item}.")
    }
}

fn take_command(game: &mut crate::game::Game<'_>, slot: usize, count: u32) -> String {
    let Some(belt) = game.held().filter(|stack| is_belt(stack)).cloned() else {
        return "Hold the belt first.".to_owned();
    };
    let mut belt = belt;
    let Some(taken) = extract_from(game, &mut belt, slot, count) else {
        return format!("Belt slot {slot} is empty.");
    };
    let item = taken.item.to_string();
    let placed = give_to_acting(game, taken);
    *game.held_mut() = Some(belt);
    format!("Took {placed} {item} from slot {slot}.")
}

fn open_from_command(game: &mut crate::game::Game<'_>, uuid: u128) -> bool {
    // Inside `native_perform_command` the acting player's state is swapped into
    // `game`, so the held belt is read there (the hub's guest copy is empty).
    let slot = game.held_slot();
    let Some(belt) = game.held().filter(|stack| is_belt(stack)).cloned() else {
        return false;
    };
    let size = belt_size(game, &belt);
    let slots = read_slots(game, &belt, size);
    let items = menu_items(size, &slots);
    let Some(window) = game.native_open_menu(uuid, MENU_ROWS, Text::literal(TITLE), items) else {
        return false;
    };
    let mut guard = state();
    let state = guard.get_or_insert_with(State::default);
    state.open.insert(
        uuid,
        OpenBelt {
            window,
            slot,
            size,
        },
    );
    true
}

/// Takes up to `count` of `item` from the acting player's inventory.
fn take_from_acting(game: &mut crate::game::Game<'_>, item: &str, count: u32) -> u32 {
    let mut left = count;
    for slot in game
        .inventory
        .iter_mut()
        .take(36)
        .chain(std::iter::once(&mut game.offhand))
    {
        if left == 0 {
            break;
        }
        if let Some(existing) = slot
            && &*existing.item == item
        {
            let take = left.min(existing.count);
            existing.count -= take;
            left -= take;
            if existing.count == 0 {
                *slot = None;
            }
        }
    }
    count - left
}

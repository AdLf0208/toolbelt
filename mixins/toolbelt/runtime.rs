// The Tool Belt's pure storage engine: how many slots a belt has, which items
// it accepts, where an inserted stack goes, what an extraction takes, and how
// the belt's slots round-trip through the item component.
//
// Game-free: the use hook, the menu sync and the `/toolbelt` command are thin
// adapters over these functions, and this file is the standalone library's
// `cargo test` target.
//
// Read from Tool Belt 2.2.10 (`ToolBeltItem`, `ConfigData.isItemStackAllowed`,
// `BeltContainer`, `ItemContainerWrapper`). See `../FINDINGS.md`.

/// The belt item id (`ToolBeltItem`, upstream `toolbelt:belt`).
pub const BELT_ITEM: &str = "toolbelt:belt";

/// A missing `belt_size` reads as two slots; upstream clamps to 2..=9.
pub const DEFAULT_SIZE: u8 = 2;
pub const MIN_SIZE: u8 = 2;
pub const MAX_SIZE: u8 = 9;

/// `minecraft:bundle` is always refused (`ConfigData.isItemStackAllowed`).
pub const BUNDLE_ITEM: &str = "minecraft:bundle";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeltStack {
    pub item: String,
    pub count: u32,
    pub max_stack: u32,
    /// A stable fingerprint of the stack's components: two stacks only merge
    /// when item *and* components agree (`ItemStack.isSameItemSameComponents`).
    pub variant: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// `allowAllNonStackableItems`: when true (the default) stackable items are
    /// refused; only `max_stack == 1` items are let in.
    pub allow_stackable: bool,
    pub blacklist: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Insertion {
    /// The stack can top up an existing equal stack in `slot` by `room`.
    Merge { slot: usize, room: u32 },
    /// The stack fits whole in the first empty slot.
    Empty { slot: usize },
    /// No slot can take it.
    Full,
}

pub fn clamp_size(size: u8) -> u8 {
    size.clamp(MIN_SIZE, MAX_SIZE)
}

pub fn slot_count(size: u8) -> usize {
    usize::from(clamp_size(size))
}

/// `ConfigData.isItemStackAllowed`, minus the whitelist (not carried in this
/// slice): a blacklisted item, a bundle or a belt is always refused, and
/// stackable items are refused unless the policy allows them.
pub fn allows(policy: &Policy, stack: &BeltStack) -> bool {
    if stack.count == 0 {
        return true;
    }
    if stack.item == BELT_ITEM || stack.item == BUNDLE_ITEM {
        return false;
    }
    if policy.blacklist.iter().any(|id| id == &stack.item) {
        return false;
    }
    policy.allow_stackable || stack.max_stack <= 1
}

pub fn find_insertion(slots: &[Option<BeltStack>], size: u8, stack: &BeltStack) -> Insertion {
    let len = slot_count(size).min(slots.len());
    for (index, slot) in slots.iter().take(len).enumerate() {
        if let Some(existing) = slot
            && existing.item == stack.item
            && existing.variant == stack.variant
            && existing.max_stack > 0
            && existing.count < existing.max_stack
        {
            return Insertion::Merge {
                slot: index,
                room: existing.max_stack - existing.count,
            };
        }
    }
    for (index, slot) in slots.iter().take(len).enumerate() {
        if slot.is_none() {
            return Insertion::Empty { slot: index };
        }
    }
    Insertion::Full
}

#[allow(dead_code)] // used by the standalone library tests; the injected module never calls it.
pub fn extract(slots: &mut [Option<BeltStack>], slot: usize, count: u32) -> Option<BeltStack> {
    if count == 0 {
        return None;
    }
    let existing = slots.get_mut(slot)?.as_mut()?;
    let taken = count.min(existing.count);
    if taken == 0 {
        return None;
    }
    let out = BeltStack {
        count: taken,
        ..existing.clone()
    };
    existing.count -= taken;
    if existing.count == 0 {
        slots[slot] = None;
    }
    Some(out)
}

/// The occupied slots with their index — the entries an item component stores
/// (a container keeps a slot's position, not just its stack).
#[allow(dead_code)] // used by the standalone library tests; the injected module never calls it.
pub fn to_entries(slots: &[Option<BeltStack>]) -> Vec<(usize, BeltStack)> {
    slots
        .iter()
        .enumerate()
        .filter_map(|(slot, stack)| stack.clone().map(|stack| (slot, stack)))
        .collect()
}

/// Reads the stored entries back by slot, padding to the belt's slot count;
/// entries past the size are dropped (upstream `setBeltSize` keeps only the
/// first `min(old,new)`).
#[allow(dead_code)] // used by the standalone library tests; the injected module never calls it.
pub fn from_entries(entries: &[(usize, BeltStack)], size: u8) -> Vec<Option<BeltStack>> {
    let mut slots: Vec<Option<BeltStack>> = vec![None; slot_count(size)];
    for (index, entry) in entries {
        let Some(slot) = slots.get_mut(*index) else {
            continue;
        };
        *slot = Some(BeltStack {
            count: entry.count.max(1),
            ..entry.clone()
        });
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: &str, count: u32, max_stack: u32) -> BeltStack {
        BeltStack {
            item: item.to_owned(),
            count,
            max_stack,
            variant: 0,
        }
    }

    fn variant(item: &str, count: u32, max_stack: u32, variant: u64) -> BeltStack {
        BeltStack {
            variant,
            ..stack(item, count, max_stack)
        }
    }

    #[test]
    fn a_belt_has_two_to_nine_slots() {
        assert_eq!(clamp_size(0), MIN_SIZE);
        assert_eq!(clamp_size(2), 2);
        assert_eq!(clamp_size(9), 9);
        assert_eq!(clamp_size(40), MAX_SIZE);
        assert_eq!(slot_count(DEFAULT_SIZE), 2);
        assert_eq!(slot_count(9), 9);
    }

    #[test]
    fn the_default_policy_takes_tools_and_refuses_stackable_items() {
        let policy = Policy::default();
        assert!(allows(&policy, &stack("minecraft:iron_pickaxe", 1, 1)));
        assert!(allows(&policy, &stack("minecraft:diamond_sword", 1, 1)));
        assert!(!allows(&policy, &stack("minecraft:stone", 1, 64)));
        assert!(!allows(&policy, &stack("minecraft:arrow", 1, 64)));
    }

    #[test]
    fn the_policy_may_allow_stacking_but_not_belts_or_bundles() {
        let permissive = Policy {
            allow_stackable: true,
            blacklist: Vec::new(),
        };
        assert!(allows(&permissive, &stack("minecraft:stone", 64, 64)));
        assert!(!allows(&permissive, &stack(BELT_ITEM, 1, 1)));
        assert!(!allows(&permissive, &stack(BUNDLE_ITEM, 1, 1)));
    }

    #[test]
    fn a_blacklisted_tool_is_refused() {
        let policy = Policy {
            allow_stackable: false,
            blacklist: vec!["minecraft:iron_pickaxe".to_owned()],
        };
        assert!(!allows(&policy, &stack("minecraft:iron_pickaxe", 1, 1)));
        assert!(allows(&policy, &stack("minecraft:iron_axe", 1, 1)));
    }

    #[test]
    fn insertion_merges_an_equal_stack_with_room() {
        let mut slots = vec![None; 2];
        slots[0] = Some(stack("minecraft:arrow", 10, 64));
        let insertion = find_insertion(&slots, 2, &stack("minecraft:arrow", 5, 64));
        assert_eq!(insertion, Insertion::Merge { slot: 0, room: 54 });
    }

    #[test]
    fn a_different_component_variant_does_not_merge() {
        let mut slots = vec![None; 2];
        slots[0] = Some(variant("minecraft:diamond_pickaxe", 1, 1, 7));
        let insertion = find_insertion(&slots, 2, &variant("minecraft:diamond_pickaxe", 1, 1, 9));
        assert_eq!(insertion, Insertion::Empty { slot: 1 });
    }

    #[test]
    fn insertion_uses_the_first_empty_slot_then_reports_full() {
        let first = find_insertion(&[None, None], 2, &stack("minecraft:iron_axe", 1, 1));
        assert_eq!(first, Insertion::Empty { slot: 0 });
        let full = vec![
            Some(stack("minecraft:iron_axe", 1, 1)),
            Some(stack("minecraft:iron_pickaxe", 1, 1)),
        ];
        assert_eq!(
            find_insertion(&full, 2, &stack("minecraft:iron_shovel", 1, 1)),
            Insertion::Full
        );
    }

    #[test]
    fn extraction_takes_up_to_the_count_and_keeps_the_rest() {
        let mut slots = vec![None; 2];
        slots[0] = Some(stack("minecraft:arrow", 10, 64));
        let taken = extract(&mut slots, 0, 3).expect("three arrows");
        assert_eq!(taken.count, 3);
        assert_eq!(slots[0].as_ref().expect("seven left").count, 7);
        let all = extract(&mut slots, 0, 64).expect("the rest");
        assert_eq!(all.count, 7);
        assert_eq!(slots[0], None);
        assert_eq!(extract(&mut slots, 0, 1), None);
    }

    #[test]
    fn the_slots_round_trip_through_the_component_shape() {
        let slots = vec![
            Some(stack("minecraft:iron_pickaxe", 1, 1)),
            None,
            Some(variant("minecraft:paper", 3, 64, 42)),
        ];
        let entries = to_entries(&slots);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, 0);
        assert_eq!(entries[1].0, 2);
        assert_eq!(entries[0].1.item, "minecraft:iron_pickaxe");
        assert_eq!(entries[0].1.count, 1);
        assert_eq!(from_entries(&entries, 3), slots);
        // A shorter belt drops the slots past its size.
        assert_eq!(from_entries(&entries, 2), {
            let mut trimmed = slots.clone();
            trimmed.truncate(2);
            trimmed
        });
    }

    #[test]
    fn a_full_belt_refuses_a_third_distinct_tool() {
        let mut slots: Vec<Option<BeltStack>> = (0..slot_count(2)).map(|_| None).collect();
        let tools = [
            "minecraft:iron_pickaxe",
            "minecraft:iron_axe",
            "minecraft:iron_shovel",
        ];
        assert_eq!(
            find_insertion(&slots, 2, &stack(tools[0], 1, 1)),
            Insertion::Empty { slot: 0 }
        );
        slots[0] = Some(stack(tools[0], 1, 1));
        assert_eq!(
            find_insertion(&slots, 2, &stack(tools[1], 1, 1)),
            Insertion::Empty { slot: 1 }
        );
        slots[1] = Some(stack(tools[1], 1, 1));
        assert_eq!(
            find_insertion(&slots, 2, &stack(tools[2], 1, 1)),
            Insertion::Full
        );
    }
}

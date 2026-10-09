/// Stable handle to a node stored in a [`Document`](super::Document).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub(super) slot: u32,
    pub(super) generation: u32,
}

impl NodeId {
    pub fn slot(self) -> u32 {
        self.slot
    }

    pub fn generation(self) -> u32 {
        self.generation
    }
}

impl std::fmt::Debug for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodeId({}:{})", self.slot, self.generation)
    }
}

#[derive(Debug)]
struct Slot<T> {
    /// distinguish between different nodes successively in the same slot
    generation: u32,
    /// one-way linked list
    next_free: Option<u32>,
    /// actual stored data
    value: Option<T>,
}

#[derive(Debug)]
pub(super) struct Arena<T> {
    /// storage
    slots: Vec<Slot<T>>,
    /// first free slot
    free_head: Option<u32>,
    /// Generation floor for slots recreated after trimming a vacant tail.
    generation_floor: u32,
    /// number of living objects
    len: usize,
}

impl<T> Arena<T> {
    pub(super) fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            generation_floor: 0,
            len: 0,
        }
    }

    pub(super) fn insert_with(&mut self, make_value: impl FnOnce(NodeId) -> T) -> NodeId {
        let id = if let Some(slot_index) = self.free_head {
            let slot = &mut self.slots[slot_index as usize];
            self.free_head = slot.next_free.take();
            NodeId {
                slot: slot_index,
                generation: slot.generation,
            }
        } else {
            let slot = u32::try_from(self.slots.len()).expect("document contains too many nodes");
            // if there a no free slots, create a new
            self.slots.push(Slot {
                generation: self.generation_floor,
                next_free: None,
                value: None,
            });
            NodeId {
                slot,
                generation: self.generation_floor,
            }
        };

        self.slots[id.slot as usize].value = Some(make_value(id));
        self.len += 1;
        id
    }

    /// Release a large vacant tail without changing any surviving node ID.
    pub(super) fn trim_unused_tail(&mut self) {
        // FIXME: unvalidated heuristic parameters
        if self.len.saturating_mul(2) >= self.slots.len() {
            return;
        }

        // Walk the vacant tail once, tracking the generation floor that keeps
        // stale IDs invalid. Retired slots stay as tombstones.
        let mut end = self.slots.len();
        let mut floor = self.generation_floor;
        while end > 0 {
            let slot = &self.slots[end - 1];
            if slot.value.is_some() || slot.generation == u32::MAX {
                break;
            }

            floor = floor.max(slot.generation);
            end -= 1;
        }

        if self.slots.len() - end < self.slots.len() / 4 {
            return;
        }

        self.generation_floor = floor;
        self.slots.truncate(end);
        self.free_head = None;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.value.is_none() && slot.generation != u32::MAX {
                slot.next_free = self.free_head;
                self.free_head = Some(index as u32);
            }
        }
        self.slots.shrink_to_fit();
    }

    pub(super) fn get(&self, id: NodeId) -> Option<&T> {
        let slot = self.slots.get(id.slot as usize)?;
        if slot.generation == id.generation {
            slot.value.as_ref()
        } else {
            None
        }
    }

    pub(super) fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        if slot.generation == id.generation {
            slot.value.as_mut()
        } else {
            None
        }
    }

    pub(super) fn remove(&mut self, id: NodeId) -> Option<T> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        if slot.generation != id.generation {
            // generation must be match
            return None;
        }

        let value = slot.value.take()?;
        self.len -= 1;

        // increase generation. retire it if generation got the u32::MAX.
        if let Some(next_generation) = slot.generation.checked_add(1) {
            // insert the empty slot to the head of linked list.
            slot.generation = next_generation;
            slot.next_free = self.free_head;
            self.free_head = Some(id.slot);
        }

        Some(value)
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::super::DocumentNode;
    use super::*;

    #[test]
    fn stale_id_cannot_access_reused_slot() {
        let mut arena = Arena::new();
        let old = arena.insert_with(|_| "old");
        assert_eq!(arena.remove(old), Some("old"));

        let new = arena.insert_with(|_| "new");
        assert_eq!(old.slot(), new.slot());
        assert_ne!(old.generation(), new.generation());
        assert_eq!(arena.get(old), None);
        assert_eq!(arena.get(new), Some(&"new"));
    }

    #[test]
    fn maximum_generation_slot_is_retired_instead_of_wrapping() {
        let old = NodeId {
            slot: 0,
            generation: u32::MAX,
        };
        let mut arena = Arena {
            slots: vec![Slot {
                generation: u32::MAX,
                next_free: None,
                value: Some("old"),
            }],
            free_head: None,
            generation_floor: 0,
            len: 1,
        };

        assert_eq!(arena.remove(old), Some("old"));
        let new = arena.insert_with(|_| "new");

        assert_eq!(new.slot(), 1);
        assert_eq!(arena.get(old), None);
    }

    #[test]
    fn trimmed_slots_do_not_resurrect_stale_ids() {
        let mut arena = Arena::new();
        let kept = arena.insert_with(|_| "kept");
        let removed: Vec<_> = (0..32)
            .map(|_| arena.insert_with(|_| "temporary"))
            .collect();
        for &id in &removed {
            arena.remove(id).unwrap();
        }
        arena.trim_unused_tail();
        assert_eq!(arena.slots.len(), 1);
        assert_eq!(arena.get(kept), Some(&"kept"));
        for old in removed {
            let new = arena.insert_with(|_| "new");
            assert_eq!(new.slot(), old.slot());
            assert_ne!(new.generation(), old.generation());
            assert!(arena.get(old).is_none());
        }
    }

    #[test]
    fn trimming_preserves_retired_slots() {
        let mut arena = Arena::new();
        let ids: Vec<_> = (0..32).map(|_| arena.insert_with(|_| "value")).collect();
        arena.slots[ids[1].slot() as usize].generation = u32::MAX;
        arena
            .remove(NodeId {
                slot: ids[1].slot(),
                generation: u32::MAX,
            })
            .unwrap();
        for &id in &ids[2..] {
            arena.remove(id).unwrap();
        }
        arena.trim_unused_tail();
        assert_eq!(arena.slots.len(), 2);
        let new = arena.insert_with(|_| "new");
        assert_eq!(new.slot(), 2);
        assert!(arena.get(ids[2]).is_none());
    }

    #[test]
    fn trimming_rebuilds_free_links_inside_the_surviving_prefix() {
        let mut arena = Arena::new();
        let ids: Vec<_> = (0..32).map(|_| arena.insert_with(|_| "value")).collect();
        for (index, &id) in ids.iter().enumerate() {
            if index != 0 && index != 4 {
                arena.remove(id).unwrap();
            }
        }
        arena.trim_unused_tail();
        assert_eq!(arena.slots.len(), 5);
        for index in (1..4).rev() {
            let new = arena.insert_with(|_| "new");
            assert_eq!(new.slot(), ids[index].slot());
            assert!(arena.get(ids[index]).is_none());
        }
        assert_eq!(arena.get(ids[4]), Some(&"value"));
        let appended = arena.insert_with(|_| "appended");
        assert_eq!(appended.slot(), 5);
        assert!(arena.get(ids[5]).is_none());
    }

    #[test]
    fn layout_sizes_are_visible_to_the_arena_design() {
        // Keep this measurement close to the storage definition so future
        // layout changes cannot happen without an explicit review point.
        eprintln!(
            "DocumentNode={} Slot<DocumentNode>={}",
            size_of::<DocumentNode>(),
            size_of::<Slot<DocumentNode>>()
        );
        assert!(size_of::<Slot<DocumentNode>>() <= 256);
    }
}

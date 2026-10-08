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
    /// number of living objects
    pub(super) len: usize,
}

impl<T> Arena<T> {
    pub(super) fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
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
                generation: 0,
                next_free: None,
                value: None,
            });
            NodeId {
                slot,
                generation: 0,
            }
        };

        self.slots[id.slot as usize].value = Some(make_value(id));
        self.len += 1;
        id
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
            len: 1,
        };

        assert_eq!(arena.remove(old), Some("old"));
        let new = arena.insert_with(|_| "new");

        assert_eq!(new.slot(), 1);
        assert_eq!(arena.get(old), None);
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

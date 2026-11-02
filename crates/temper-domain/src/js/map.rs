//! JS `Map` and `Set` iteration order with hashed lookups.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;

/// A JS `Map` (a `Set` with `()` values): it iterates in insertion order, and `set` on a key
/// already present replaces the value and keeps the key's place.
///
/// Each key is stored once, beside its insertion position; [`JsMap::into_vec`] sorts by that
/// position once, so building is linear in the number of `set` calls plus one sort.
#[derive(Debug)]
pub(crate) struct JsMap<K, V> {
    slots: HashMap<K, (usize, V)>,
}

impl<K: Eq + Hash, V> JsMap<K, V> {
    pub(crate) fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    /// JS `map.set(key, value)`.
    pub(crate) fn set(&mut self, key: K, value: V) {
        let next = self.slots.len();
        match self.slots.entry(key) {
            Entry::Occupied(mut slot) => slot.get_mut().1 = value,
            Entry::Vacant(slot) => {
                slot.insert((next, value));
            }
        }
    }

    /// The entries in insertion order.
    pub(crate) fn into_vec(self) -> Vec<(K, V)> {
        let mut entries: Vec<_> = self.slots.into_iter().collect();
        entries.sort_unstable_by_key(|(_, (at, _))| *at);
        entries.into_iter().map(|(k, (_, v))| (k, v)).collect()
    }
}

impl<K: Eq + Hash> JsMap<K, ()> {
    /// JS `set.add(key)`.
    pub(crate) fn add(&mut self, key: K) {
        self.set(key, ());
    }

    /// The keys in insertion order.
    pub(crate) fn into_keys(self) -> Vec<K> {
        self.into_vec().into_iter().map(|(k, ())| k).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::JsMap;

    #[test]
    fn set_keeps_a_repeated_keys_place_and_takes_its_new_value() {
        let mut map = JsMap::new();
        for (k, v) in [("b", 1), ("a", 2), ("c", 3), ("b", 4), ("a", 5)] {
            map.set(k, v);
        }
        assert_eq!(map.into_vec(), [("b", 4), ("a", 5), ("c", 3)]);
    }

    #[test]
    fn add_keeps_the_first_insertion_order() {
        let mut set = JsMap::new();
        for k in [3, 1, 3, 2, 1] {
            set.add(k);
        }
        assert_eq!(set.into_keys(), [3, 1, 2]);
    }
}

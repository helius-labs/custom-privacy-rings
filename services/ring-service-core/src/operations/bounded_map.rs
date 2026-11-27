//! A map that holds at most a bound of entries, evicting the oldest insert.

use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

/// A miss cannot tell an evicted key from an absent one, the caller asks storage.
pub struct BoundedMap<K, V> {
    entries: HashMap<K, V>,
    /// Insert order, the front leaves first once the map holds more than its bound.
    order: VecDeque<K>,
    bound: usize,
}

impl<K: Copy + Eq + Hash, V: Copy> BoundedMap<K, V> {
    pub fn bounded(bound: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bound,
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.entries.insert(key, value).is_some() {
            return;
        }
        self.order.push_back(key);
        while self.entries.len() > self.bound {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter()
    }
}

impl<K: Copy + Eq + Hash, V: Copy> Extend<(K, V)> for BoundedMap<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, entries: I) {
        for (key, value) in entries {
            self.insert(key, value);
        }
    }
}

//! A map that either holds every entry or at most a bound of them, evicting the oldest insert.

use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

/// What a lookup knows, a bounded map cannot tell an evicted key from an absent one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cached<V> {
    Hit(V),
    Absent,
    Unknown,
}

pub struct BoundedMap<K, V> {
    entries: HashMap<K, V>,
    /// Insert order, the front leaves first once the map holds more than its bound.
    order: VecDeque<K>,
    bound: Option<usize>,
}

impl<K, V> Default for BoundedMap<K, V> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bound: None,
        }
    }
}

impl<K: Copy + Eq + Hash, V: Copy> BoundedMap<K, V> {
    pub fn bounded(bound: usize) -> Self {
        Self {
            bound: Some(bound),
            ..Self::default()
        }
    }

    pub const fn is_complete(&self) -> bool {
        self.bound.is_none()
    }

    pub fn cached(&self, key: &K) -> Cached<V> {
        match (self.entries.get(key), self.bound) {
            (Some(value), _) => Cached::Hit(*value),
            (None, None) => Cached::Absent,
            (None, Some(_)) => Cached::Unknown,
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
        let Some(bound) = self.bound else {
            return;
        };
        while self.entries.len() > bound {
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

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.values()
    }
}

impl<K: Copy + Eq + Hash, V: Copy> Extend<(K, V)> for BoundedMap<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, entries: I) {
        for (key, value) in entries {
            self.insert(key, value);
        }
    }
}

use std::collections::VecDeque;

/// A small bounded LRU. Entries live only in memory and never enter diagnostics.
pub struct Cache {
    entries: VecDeque<(String, String)>,
    capacity: usize,
}
impl Cache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
        }
    }
    pub fn get(&mut self, key: &str) -> Option<String> {
        let i = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(i)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }
    pub fn insert(&mut self, key: String, value: String) {
        if self.capacity == 0 {
            return;
        }
        self.entries.retain(|(k, _)| k != &key);
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((key, value));
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lru_is_bounded_and_refreshes() {
        let mut c = Cache::new(2);
        c.insert("a".into(), "A".into());
        c.insert("b".into(), "B".into());
        assert_eq!(c.get("a"), Some("A".into()));
        c.insert("c".into(), "C".into());
        assert_eq!(c.get("b"), None);
        c.clear();
        assert_eq!(c.get("a"), None);
    }
}

use std::num::NonZeroUsize;

use lru::LruCache;

use super::request::DecodedImage;

/// Caches decoded, already-downscaled images by their position in the
/// review stream. Bounded by both entry count and an approximate byte
/// budget, since a handful of full-viewport images can already add up.
pub struct DecodedCache {
    entries: LruCache<usize, DecodedImage>,
    byte_budget: usize,
    bytes_used: usize,
}

impl DecodedCache {
    pub fn new(capacity: usize, byte_budget: usize) -> Self {
        Self {
            entries: LruCache::new(NonZeroUsize::new(capacity.max(1)).unwrap()),
            byte_budget,
            bytes_used: 0,
        }
    }

    pub fn get(&mut self, index: usize) -> Option<&DecodedImage> {
        self.entries.get(&index)
    }

    pub fn contains(&self, index: usize) -> bool {
        self.entries.contains(&index)
    }

    pub fn insert(&mut self, index: usize, image: DecodedImage) {
        let size = image.approx_bytes();
        if let Some(old) = self.entries.push(index, image).map(|(_, v)| v) {
            self.bytes_used = self.bytes_used.saturating_sub(old.approx_bytes());
        }
        self.bytes_used += size;

        while self.bytes_used > self.byte_budget && self.entries.len() > 1 {
            if let Some((_, evicted)) = self.entries.pop_lru() {
                self.bytes_used = self.bytes_used.saturating_sub(evicted.approx_bytes());
            } else {
                break;
            }
        }
    }

    /// Drop any cached entry outside `keep_range`, except `protect`
    /// (the index currently on screen). Used after a non-contiguous jump
    /// so a stale prefetch window doesn't linger in memory.
    pub fn evict_outside(&mut self, keep_range: std::ops::Range<usize>, protect: usize) {
        let stale: Vec<usize> = self
            .entries
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| *k != protect && !keep_range.contains(k))
            .collect();
        for key in stale {
            if let Some(evicted) = self.entries.pop(&key) {
                self.bytes_used = self.bytes_used.saturating_sub(evicted.approx_bytes());
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(bytes: usize) -> DecodedImage {
        DecodedImage {
            width: 1,
            height: 1,
            rgba: vec![0u8; bytes],
        }
    }

    #[test]
    fn evicts_by_byte_budget() {
        let mut cache = DecodedCache::new(100, 25);
        cache.insert(0, img(10));
        cache.insert(1, img(10));
        cache.insert(2, img(10));
        // budget is 25 bytes; inserting a third 10-byte entry must evict the LRU one
        assert!(cache.len() <= 2);
        assert!(cache.contains(2));
    }

    #[test]
    fn evict_outside_keeps_protected_index() {
        let mut cache = DecodedCache::new(100, 10_000);
        cache.insert(5, img(1));
        cache.insert(50, img(1));
        cache.evict_outside(0..10, 50);
        assert!(cache.contains(5));
        assert!(cache.contains(50)); // protected even though outside range
    }
}

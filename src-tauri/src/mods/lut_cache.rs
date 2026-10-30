//! The LUTs kept in memory, by path, the oldest dropped first: the shape of
//! `DecodedImageCache` (cache_utils.rs). Ours.

use std::sync::Arc;

use crate::lut_processing::Lut;

/// Most LUTs held at once: of 64 steps, 3.1 MB each, about 100 MB.
pub const MAX_CACHED_LUTS: usize = 32;

/// Least recently used first.
#[derive(Default)]
pub struct LutCache {
    items: Vec<(String, Arc<Lut>)>,
}

impl LutCache {
    pub fn get(&mut self, path: &str) -> Option<Arc<Lut>> {
        let pos = self.items.iter().position(|(p, _)| p == path)?;
        let entry = self.items.remove(pos);
        let lut = Arc::clone(&entry.1);
        self.items.push(entry);
        Some(lut)
    }

    /// `path` holds `lut` from now on, so a rewritten file takes effect.
    pub fn insert(&mut self, path: String, lut: Arc<Lut>) {
        if let Some(pos) = self.items.iter().position(|(p, _)| *p == path) {
            self.items.remove(pos);
        } else if self.items.len() >= MAX_CACHED_LUTS {
            self.items.remove(0);
        }
        self.items.push((path, lut));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lut() -> Arc<Lut> {
        Arc::new(Lut {
            size: 2,
            data: vec![0.0; 24],
        })
    }

    fn name(n: usize) -> String {
        format!("{n}.cube")
    }

    fn full() -> LutCache {
        let mut cache = LutCache::default();
        for n in 0..MAX_CACHED_LUTS {
            cache.insert(name(n), lut());
        }
        cache
    }

    /// North star 5: however many LUTs pass through, 64-step ones hold no
    /// more than this.
    #[test]
    fn a_hundred_luts_hold_about_100_mb() {
        let mut cache = LutCache::default();
        for n in 0..100 {
            let data = vec![0.0; 64usize.pow(3) * 3];
            cache.insert(name(n), Arc::new(Lut { size: 64, data }));
        }
        let bytes: usize = cache
            .items
            .iter()
            .map(|(_, lut)| size_of_val(lut.data.as_slice()))
            .sum();
        assert!(bytes <= 101_000_000, "{bytes} bytes");
    }

    #[test]
    fn past_the_bound_the_least_recently_used_goes() {
        let mut cache = full();
        assert!(
            cache.get(&name(0)).is_some(),
            "used, so 1 is now the oldest"
        );
        cache.insert(name(MAX_CACHED_LUTS), lut());
        assert!(cache.get(&name(1)).is_none());
        assert!(cache.get(&name(0)).is_some());
        assert!(cache.get(&name(MAX_CACHED_LUTS)).is_some());
    }

    #[test]
    fn a_rewritten_file_replaces_its_path_and_nothing_else() {
        let mut cache = full();
        let rewritten = lut();
        cache.insert(name(5), Arc::clone(&rewritten));
        assert!(Arc::ptr_eq(&cache.get(&name(5)).unwrap(), &rewritten));
        for n in 0..MAX_CACHED_LUTS {
            assert!(cache.get(&name(n)).is_some(), "{n} stays");
        }
    }
}

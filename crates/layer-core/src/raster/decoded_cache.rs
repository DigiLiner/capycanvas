//! Bounded, disposable exact samples. The compressed immutable tile remains the
//! authority; cache keys never keep document/history backing alive.
use super::TileBlob;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecodedTileCacheStats {
    /// Sample allocation capacities; map/Arc metadata and decode scratch are
    /// separate. A caller can briefly retain one evicted tile during upload.
    pub resident_bytes: usize,
    pub peak_bytes: usize,
    pub limit_bytes: usize,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

struct Entry {
    tile: Weak<TileBlob>,
    samples: Arc<crate::memory::DecodedBytes>,
    used: u64,
}
#[derive(Default)]
struct State {
    entries: HashMap<u64, Entry>,
    clock: u64,
    stats: DecodedTileCacheStats,
}
pub struct DecodedTileCache {
    state: Mutex<State>,
}
static SHARED:std::sync::OnceLock<Arc<DecodedTileCache>>=std::sync::OnceLock::new();
impl DecodedTileCache {
    pub fn shared()->Arc<Self> {
        SHARED.get_or_init(||Arc::new(Self::new(512*1024*1024))).clone()
    }
    pub(crate) fn collect_shared()->Result<(),String> {
        let Some(cache)=SHARED.get() else {return Ok(());};
        let mut state=cache.state.lock().map_err(|_|"Source sample cache failed")?;
        let mut retired=0;let count=state.entries.len();
        state.entries.retain(|_,entry| {
            if entry.tile.strong_count()!=0 {true}else{retired+=entry.samples.capacity();false}
        });
        state.stats.resident_bytes-=retired;state.stats.evictions+=(count-state.entries.len()) as u64;Ok(())
    }
    pub(crate) fn evict_shared() {
        let Some(cache)=SHARED.get() else {return;};
        if let Ok(mut state)=cache.state.try_lock() {
            state.stats.evictions+=state.entries.len() as u64;
            state.entries.clear();state.stats.resident_bytes=0;
        }
    }
    pub fn new(limit_bytes: usize) -> Self {
        Self {
            state: Mutex::new(State {
                stats: DecodedTileCacheStats {
                    limit_bytes,
                    ..Default::default()
                },
                ..Default::default()
            }),
        }
    }

    pub fn stats(&self) -> DecodedTileCacheStats {
        let state = self.state.lock().unwrap();
        DecodedTileCacheStats {
            entries: state.entries.len(),
            ..state.stats
        }
    }

    pub fn decode(&self, tile: &Arc<TileBlob>) -> Result<Arc<crate::memory::DecodedBytes>, super::TileError> {
        let key = tile.owner_identity();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Source sample cache failed")?;
            state.stats.limit_bytes=state.stats.limit_bytes.min(crate::memory::images().maximum()/8);
            state.clock = state.clock.saturating_add(1);
            let used = state.clock;
            if let Some(entry) = state.entries.get_mut(&key) {
                entry.used = used;
                let samples = entry.samples.clone();
                state.stats.hits += 1;
                return Ok(samples);
            }
            state.stats.misses += 1;
            let bytes=tile.descriptor.byte_len([super::TILE_SIZE;2]).ok_or("Invalid source tile representation")?;
            if bytes<=state.stats.limit_bytes {
                while state.stats.resident_bytes>state.stats.limit_bytes-bytes {Self::evict(&mut state);}
            }
        }

        // Never hold the cache lock across decompression or integrity checking.
        // Concurrent misses may do one tile of duplicate work, rather than
        // making a canvas wait on a file worker's decode.
        let samples = Arc::new(tile.decode()?);
        let bytes = samples.capacity();
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Source sample cache failed")?;
        if let Some(entry) = state.entries.get(&key) {
            return Ok(entry.samples.clone());
        }
        if bytes > state.stats.limit_bytes {
            return Ok(samples);
        }
        while state.stats.resident_bytes > state.stats.limit_bytes - bytes {
            // Expired sources retire first; otherwise evict the least recently
            // requested tile. Active readers retain their immutable samples.
            Self::evict(&mut state);
        }
        state.clock = state.clock.saturating_add(1);
        let used = state.clock;
        state.entries.insert(
            key,
            Entry {
                tile: Arc::downgrade(tile),
                samples: samples.clone(),
                used,
            },
        );
        state.stats.resident_bytes += bytes;
        state.stats.peak_bytes = state.stats.peak_bytes.max(state.stats.resident_bytes);
        Ok(samples)
    }
    fn evict(state:&mut State) {
        let oldest=*state.entries.iter().min_by_key(|(_,entry)|(entry.tile.strong_count()!=0,entry.used)).unwrap().0;
        let entry=state.entries.remove(&oldest).unwrap();
        state.stats.resident_bytes-=entry.samples.capacity();state.stats.evictions+=1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DocumentColor, SampleDepth, PixelDescriptor, RgbSpace};

    fn tile(value: u8) -> Arc<TileBlob> {
        Arc::new(TileBlob::encode(PixelDescriptor::COVERAGE8, &vec![value; 65536]).unwrap())
    }

    #[test]
    fn shared_readers_reuse_samples_and_pending_uploads_outlive_source_retirement() {
        let a=DecodedTileCache::shared();let b=DecodedTileCache::shared();let source=tile(37);
        let first=a.decode(&source).unwrap();let second=b.decode(&source).unwrap();
        assert!(Arc::ptr_eq(&first,&second));let pending=Arc::downgrade(&first);
        drop(source);crate::memory::images().collect().unwrap();
        assert!(first.iter().all(|byte|*byte==37));drop(first);assert!(pending.upgrade().is_some());
        drop(second);assert!(pending.upgrade().is_none());
    }

    #[test]
    fn cache_evicts_within_budget_without_changing_active_readers_or_owning_sources() {
        let cache = DecodedTileCache::new(2 * 65536);
        let a = tile(13);
        let b = tile(71);
        let c = tile(191);
        let first = cache.decode(&a).unwrap();
        cache.decode(&b).unwrap();
        assert!(Arc::ptr_eq(&cache.decode(&a).unwrap(), &first));
        cache.decode(&c).unwrap(); // B is least recently used.
        assert_eq!(cache.stats().evictions, 1);
        assert!(Arc::ptr_eq(&cache.decode(&a).unwrap(), &first));
        let weak = Arc::downgrade(&a);
        assert_eq!(Arc::strong_count(&a), 1);
        drop(a);
        assert!(weak.upgrade().is_none());
        cache.decode(&b).unwrap(); // Expired A retires before still-live C.
        assert_eq!(cache.stats().evictions, 2);
        assert!(first.iter().all(|&v| v == 13));
        assert_eq!(cache.stats().resident_bytes, 2 * 65536);
        assert_eq!(cache.stats().peak_bytes, 2 * 65536);
        assert_eq!(cache.stats().entries, 2);
    }

    #[test]
    fn cache_keeps_exact_u16_samples_and_rejects_corrupt_tiles() {
        let color = DocumentColor {
            space: RgbSpace::ProPhoto,
            depth: SampleDepth::U16,
        };
        let bytes: Vec<_> = (0..65536u32)
            .flat_map(|i| {
                [i as u16, (i as u16).wrapping_mul(107), 17001, 49157]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
            })
            .collect();
        let good = Arc::new(TileBlob::encode(color.paint_descriptor(), &bytes).unwrap());
        let mut bad = TileBlob::encode(color.paint_descriptor(), &bytes).unwrap();
        bad.compressed = Arc::new(Arc::<[u8]>::from([0u8]).into());
        let cache = DecodedTileCache::new(bytes.len());
        assert_eq!(cache.decode(&good).unwrap().as_slice(), bytes);
        assert!(cache.decode(&Arc::new(bad)).is_err());
        assert_eq!(cache.decode(&good).unwrap().as_slice(), bytes);
        let disabled = DecodedTileCache::new(0);
        assert_eq!(disabled.decode(&good).unwrap().as_slice(), bytes);
        assert_eq!(disabled.stats().resident_bytes, 0);
    }

    #[test]
    fn distinct_owners_with_the_same_resource_id_have_independent_cache_entries() {
        let first = tile(23);
        let restored = Arc::new(TileBlob::from_package(first.resource_id(), first.descriptor, first.compressed().unwrap()).unwrap());
        assert_ne!(first.owner_identity(), restored.owner_identity());
        assert_eq!(first.owner_identity(), first.clone().owner_identity());
        let cache = DecodedTileCache::new(2 * 65536);
        let a = cache.decode(&first).unwrap();
        let b = cache.decode(&restored).unwrap();
        assert_eq!(a, b);
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(cache.stats().misses, 2);
        assert!(Arc::ptr_eq(&a, &cache.decode(&first).unwrap()));
        let owner = Arc::downgrade(&first);
        drop(first);
        assert!(owner.upgrade().is_none());
    }

    #[test]
    fn concurrent_readers_share_retained_tiles_and_remain_correct_during_eviction() {
        let cache = Arc::new(DecodedTileCache::new(2 * 65536));
        let tiles: Vec<_> = (0..8).map(tile).collect();
        let barrier = Arc::new(std::sync::Barrier::new(4));
        std::thread::scope(|scope| {
            for offset in 0..4 {
                let cache = &cache;
                let tiles = &tiles;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    for i in 0..32 {
                        let index = (i + offset) % tiles.len();
                        let samples = cache.decode(&tiles[index]).unwrap();
                        assert!(samples.iter().all(|&v| v == index as u8));
                        assert!(cache.stats().resident_bytes <= 2 * 65536);
                    }
                });
            }
        });
        let sample = cache.decode(&tiles[0]).unwrap();
        assert!(Arc::ptr_eq(&sample, &cache.decode(&tiles[0]).unwrap()));
        assert!(cache.stats().evictions > 0);
        assert!(cache.stats().peak_bytes <= 2 * 65536);
    }
}

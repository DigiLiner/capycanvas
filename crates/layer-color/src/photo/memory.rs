//! Admission budgets from remaining memory, shared by every platform.
//! These are snapshots, not reservations or whole-editor memory guarantees.

/// Host-supplied memory must be bytes still available to this process, after
/// existing documents, GPU allocations and host-specific limits are accounted
/// for. Browsers without a reliable query use the same conservative fallback as
/// any native host where the query fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhotoMemoryBudget {
    pub source_bytes: usize,
    pub decode_bytes: usize,
    pub encode_bytes: usize,
}

impl PhotoMemoryBudget {
    pub fn from_available_memory(bytes: u64) -> Self {
        // A single Rust allocation may not exceed isize::MAX. This also keeps
        // 32-bit/WASM arithmetic within its address space even on a large host.
        let bytes = bytes.min(isize::MAX as u64) as usize;
        Self {
            source_bytes: bytes / 8,
            decode_bytes: bytes / 3,
            encode_bytes: bytes / 2,
        }
    }

    /// Current process/system headroom when the host can measure it.
    pub fn available_memory() -> Option<u64> {
        layer_core::memory::available_memory()
    }

    pub fn current() -> Self {
        let _=layer_core::memory::images().collect();
        Self::from_available_memory(Self::available_memory().unwrap_or(512 * 1024 * 1024))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budgets_scale_with_headroom_and_never_invent_a_minimum() {
        for available in [0, 1, 1024, 128 << 20, 512 << 20, 4 << 30, 8 << 30, u64::MAX] {
            let b = PhotoMemoryBudget::from_available_memory(available);
            let usable = available.min(isize::MAX as u64) as usize;
            assert_eq!(b.source_bytes, usable / 8);
            assert_eq!(b.decode_bytes, usable / 3);
            assert_eq!(b.encode_bytes, usable / 2);
            assert!(b.source_bytes + b.decode_bytes <= usable / 2);
        }
    }
}

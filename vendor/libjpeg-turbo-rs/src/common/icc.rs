// libjpeg-turbo-rs: alloc prelude (no_std support, issue #356)
use crate::common::types::IccChunk;
#[allow(unused_imports)]
use alloc::vec::Vec;
#[allow(unused_imports)]
use alloc::{format, vec};

/// Reassemble an ICC profile from APP2 marker chunks.
///
/// Validates that all chunks report the same `num_markers`, sequence numbers
/// are contiguous from 1 to `num_markers`, and there are no duplicates.
/// Returns `None` if the chunks are empty, invalid, or incomplete.
pub fn reassemble_icc_profile(chunks: &[IccChunk]) -> Option<Vec<u8>> {
    try_reassemble_icc_profile(chunks).ok().flatten()
}

pub(crate) fn try_reassemble_icc_profile(chunks: &[IccChunk]) -> crate::common::error::Result<Option<Vec<u8>>> {
    use crate::common::{error::JpegError, try_alloc::try_reserved_vec};
    let Some(first) = chunks.first() else { return Ok(None); };
    let count = first.num_markers as usize;
    if count == 0 { return Ok(None); }
    let mut slots: [Option<&IccChunk>; 256] = [None; 256];
    for chunk in chunks {
        let index = chunk.seq_no as usize;
        if chunk.num_markers as usize != count || index == 0 || index > count || slots[index].is_some() { return Ok(None); }
        slots[index] = Some(chunk);
    }
    let ordered = &slots[1..=count];
    if ordered.iter().any(Option::is_none) { return Ok(None); }
    let length = ordered.iter().flatten().try_fold(0usize, |sum, chunk| sum.checked_add(chunk.data.len()))
        .ok_or(JpegError::LimitExceeded { what: "ICC profile length", actual: u64::MAX, limit: isize::MAX as u64 })?;
    let mut profile = try_reserved_vec(length, "ICC profile")?;
    for chunk in ordered.iter().flatten() { profile.extend_from_slice(&chunk.data); }
    Ok(Some(profile))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(seq_no: u8, num_markers: u8, data: &[u8]) -> IccChunk {
        IccChunk {
            seq_no,
            num_markers,
            data: data.to_vec(),
        }
    }

    #[test]
    fn single_chunk_profile() {
        let chunks = vec![make_chunk(1, 1, &[0x00, 0x01, 0x02, 0x03])];
        let profile = reassemble_icc_profile(&chunks).unwrap();
        assert_eq!(profile, vec![0x00, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn multi_chunk_reassembles_in_seq_order() {
        // Chunks arrive out of order
        let chunks = vec![
            make_chunk(2, 3, &[0x04, 0x05]),
            make_chunk(1, 3, &[0x01, 0x02, 0x03]),
            make_chunk(3, 3, &[0x06]),
        ];
        let profile = reassemble_icc_profile(&chunks).unwrap();
        assert_eq!(profile, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
    }

    #[test]
    fn empty_chunks_returns_none() {
        assert!(reassemble_icc_profile(&[]).is_none());
    }

    #[test]
    fn duplicate_seq_no_returns_none() {
        let chunks = vec![make_chunk(1, 2, &[0x01]), make_chunk(1, 2, &[0x02])];
        assert!(reassemble_icc_profile(&chunks).is_none());
    }

    #[test]
    fn gap_in_seq_returns_none() {
        // seq 1 and 3, missing 2
        let chunks = vec![make_chunk(1, 3, &[0x01]), make_chunk(3, 3, &[0x03])];
        assert!(reassemble_icc_profile(&chunks).is_none());
    }

    #[test]
    fn inconsistent_num_markers_returns_none() {
        let chunks = vec![make_chunk(1, 2, &[0x01]), make_chunk(2, 3, &[0x02])];
        assert!(reassemble_icc_profile(&chunks).is_none());
    }

    #[test]
    fn seq_no_zero_returns_none() {
        let chunks = vec![make_chunk(0, 1, &[0x01])];
        assert!(reassemble_icc_profile(&chunks).is_none());
    }

    #[test]
    fn seq_no_exceeds_num_markers_returns_none() {
        let chunks = vec![make_chunk(2, 1, &[0x01])];
        assert!(reassemble_icc_profile(&chunks).is_none());
    }

    #[test]
    fn num_markers_zero_returns_none() {
        let chunks = vec![make_chunk(1, 0, &[0x01])];
        assert!(reassemble_icc_profile(&chunks).is_none());
    }
}

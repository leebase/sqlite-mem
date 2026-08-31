//! Brute-force cosine similarity over BLOB-stored embeddings, behind a
//! `VectorIndex` trait seam (architecture.md §13, §23) so a future ANN or
//! sqlite-vec implementation is a contained swap -- the BLOB column layout
//! does not change either way.

/// Ranks a set of candidate embeddings against a query vector.
/// Implementations must be untruncated: every candidate receives a rank
/// (architecture.md §13 -- the semantic leg protects deep-filter recall by
/// scoring the whole filtered corpus, not a pre-truncated shortlist).
pub trait VectorIndex {
    /// Returns `(id, similarity)` pairs sorted best match first. Ties are
    /// broken by ascending `id` so output order is deterministic even when
    /// two candidates score identically (architecture.md §12 determinism
    /// contract).
    fn rank(&self, query: &[f32], candidates: &[(String, Vec<f32>)]) -> Vec<(String, f32)>;
}

/// The only `VectorIndex` implementation in v1 (architecture.md §10, §13):
/// a plain linear scan computing cosine similarity. Verified fast enough at
/// memory scale (<10ms at 50K chunks per the Satchel/sqlite-graphrag
/// precedent cited in architecture.md §10) that no ANN index is justified.
pub struct BruteForceCosine;

impl VectorIndex for BruteForceCosine {
    fn rank(&self, query: &[f32], candidates: &[(String, Vec<f32>)]) -> Vec<(String, f32)> {
        let mut scored: Vec<(String, f32)> = candidates
            .iter()
            .map(|(id, v)| (id.clone(), cosine(query, v)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        scored
    }
}

/// Cosine similarity. Embeddings are already L2-normalized by the embedder
/// (see `embed::Embedder::embed`'s doc comment), so in practice this
/// reduces to a dot product, but the full formula is computed rather than
/// assumed so a malformed/corrupt embedding (mismatched dims, a zero
/// vector) degrades to a defined `0.0` instead of panicking or producing
/// nonsense from an un-normalized dot product.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f32;
    let mut norm_a = 0f32;
    let mut norm_b = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

/// Decodes a little-endian `f32` BLOB (the `chunks.embedding` column
/// format, architecture.md §10) back into a vector. A blob whose length is
/// not a multiple of 4 bytes (a corrupted/truncated embedding) simply
/// drops its trailing partial float rather than panicking -- `ask --mode
/// lexical` must keep working on a DB with unusable embeddings
/// (architecture.md §13), and this keeps the semantic leg from crashing on
/// the same DB even though its results there are undefined.
pub fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_of_identical_unit_vectors_is_one() {
        let v = vec![1.0f32, 0.0, 0.0];
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_orthogonal_vectors_is_zero() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!(cosine(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_opposite_vectors_is_negative_one() {
        let a = vec![1.0f32, 0.0];
        let b = vec![-1.0f32, 0.0];
        assert!((cosine(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_handles_mismatched_dims_as_zero_not_panic() {
        let a = vec![1.0f32, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        assert_eq!(cosine(&a, &b), 0.0);
    }

    #[test]
    fn cosine_handles_zero_vector_as_zero_not_nan() {
        let a = vec![0.0f32, 0.0];
        let b = vec![1.0f32, 0.0];
        assert_eq!(cosine(&a, &b), 0.0);
    }

    #[test]
    fn blob_round_trips_little_endian_f32() {
        let v = vec![1.0f32, -2.5, 0.125];
        let mut blob = Vec::new();
        for f in &v {
            blob.extend_from_slice(&f.to_le_bytes());
        }
        assert_eq!(blob_to_embedding(&blob), v);
    }

    #[test]
    fn blob_with_trailing_partial_float_drops_it_instead_of_panicking() {
        let mut blob = 1.0f32.to_le_bytes().to_vec();
        blob.push(0xAB); // 1 trailing byte, not a full f32
        assert_eq!(blob_to_embedding(&blob), vec![1.0f32]);
    }

    #[test]
    fn rank_orders_best_similarity_first_and_breaks_ties_by_id() {
        let query = vec![1.0f32, 0.0];
        let candidates = vec![
            ("b".to_string(), vec![1.0f32, 0.0]), // identical, sim 1.0
            ("a".to_string(), vec![1.0f32, 0.0]), // identical, sim 1.0 -- ties with "b"
            ("c".to_string(), vec![0.0f32, 1.0]), // orthogonal, sim 0.0
        ];
        let ranked = BruteForceCosine.rank(&query, &candidates);
        assert_eq!(ranked[0].0, "a"); // tie broken by ascending id
        assert_eq!(ranked[1].0, "b");
        assert_eq!(ranked[2].0, "c");
    }

    #[test]
    fn rank_is_untruncated_every_candidate_gets_a_rank() {
        let query = vec![1.0f32];
        let candidates: Vec<(String, Vec<f32>)> =
            (0..50).map(|i| (format!("id{i}"), vec![1.0f32])).collect();
        let ranked = BruteForceCosine.rank(&query, &candidates);
        assert_eq!(ranked.len(), 50);
    }
}

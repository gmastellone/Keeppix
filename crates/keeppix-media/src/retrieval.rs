//! Text-to-image retrieval metrics.
//!
//! Pure: no ONNX dependency. Used by the IT/EN bench to compare retrieval
//! quality across languages on the same set of photos.

/// Cosine similarity between L2-normalized vectors (or not: normalizes on
/// the fly).
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

/// For each query, the 1-based rank of the correct photo (index `i` in the
/// gallery).
///
/// `queries[i]` must correspond to `gallery[i]`. Returns the ranks; empty
/// vector if the gallery is empty.
///
/// # Panics
/// If `queries` and `gallery` have different lengths.
#[must_use]
pub fn retrieval_ranks(queries: &[Vec<f32>], gallery: &[Vec<f32>]) -> Vec<usize> {
    assert_eq!(queries.len(), gallery.len());
    let n = queries.len();
    let mut ranks = Vec::with_capacity(n);
    for (qi, q) in queries.iter().enumerate() {
        let mut scored: Vec<(usize, f32)> = gallery
            .iter()
            .enumerate()
            .map(|(gi, g)| (gi, cosine_similarity(q, g)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let rank = scored
            .iter()
            .position(|(gi, _)| *gi == qi)
            .map_or(n, |p| p + 1);
        ranks.push(rank);
    }
    ranks
}

/// Recall@k: fraction of queries whose correct match is within the top k.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn recall_at_k(ranks: &[usize], k: usize) -> f64 {
    if ranks.is_empty() {
        return 0.0;
    }
    let hits = ranks.iter().filter(|&&r| r <= k).count();
    hits as f64 / ranks.len() as f64
}

/// Mean Reciprocal Rank.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn mean_reciprocal_rank(ranks: &[usize]) -> f64 {
    if ranks.is_empty() {
        return 0.0;
    }
    let sum: f64 = ranks.iter().map(|&r| 1.0 / r as f64).sum();
    sum / ranks.len() as f64
}

/// Aggregate outcome for one language.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalScore {
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub mrr: f64,
    pub ranks: Vec<usize>,
}

#[must_use]
pub fn score_retrieval(queries: &[Vec<f32>], gallery: &[Vec<f32>]) -> RetrievalScore {
    let ranks = retrieval_ranks(queries, gallery);
    RetrievalScore {
        recall_at_1: recall_at_k(&ranks, 1),
        recall_at_5: recall_at_k(&ranks, 5),
        mrr: mean_reciprocal_rank(&ranks),
        ranks,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

    use super::*;

    fn v(xs: &[f32]) -> Vec<f32> {
        xs.to_vec()
    }

    #[test]
    fn cosine_of_identical_unit_vectors_is_one() {
        let a = v(&[0.6, 0.8]);
        assert!((cosine_similarity(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_orthogonal_vectors_is_zero() {
        let a = v(&[1.0, 0.0]);
        let b = v(&[0.0, 1.0]);
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn perfect_diagonal_gallery_yields_recall_one_and_mrr_one() {
        // Three queries aligned with the three images: q_i ≈ gallery_i and
        // orthogonal to the others — if the ranking is wrong, the test fails.
        let gallery = vec![
            v(&[1.0, 0.0, 0.0]),
            v(&[0.0, 1.0, 0.0]),
            v(&[0.0, 0.0, 1.0]),
        ];
        let queries = gallery.clone();
        let score = score_retrieval(&queries, &gallery);
        assert_eq!(score.ranks, vec![1, 1, 1]);
        assert_eq!(score.recall_at_1, 1.0);
        assert_eq!(score.recall_at_5, 1.0);
        assert_eq!(score.mrr, 1.0);
    }

    #[test]
    fn swapped_first_two_queries_drop_recall_at_1_but_keep_recall_at_5() {
        let gallery = vec![
            v(&[1.0, 0.0, 0.0]),
            v(&[0.0, 1.0, 0.0]),
            v(&[0.0, 0.0, 1.0]),
        ];
        // Queries 0 and 1 deliberately swapped; 2 stays aligned.
        let queries = vec![gallery[1].clone(), gallery[0].clone(), gallery[2].clone()];
        let score = score_retrieval(&queries, &gallery);
        assert_eq!(score.ranks[0], 2);
        assert_eq!(score.ranks[1], 2);
        assert_eq!(score.ranks[2], 1);
        assert!((score.recall_at_1 - (1.0 / 3.0)).abs() < 1e-9);
        assert_eq!(score.recall_at_5, 1.0);
        // MRR = (1/2 + 1/2 + 1) / 3 = 2/3
        assert!((score.mrr - (2.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn recall_at_k_is_zero_when_every_match_is_outside_k() {
        assert_eq!(recall_at_k(&[3, 4, 5], 1), 0.0);
        assert_eq!(recall_at_k(&[3, 4, 5], 2), 0.0);
        assert!((recall_at_k(&[3, 4, 5], 3) - 1.0 / 3.0).abs() < 1e-9);
    }
}

//! Reciprocal Rank Fusion, k=60 (architecture.md §12-13).
//!
//! `score(c) = Σ_legs 1/(60 + rank_leg(c))`, ranks 1-based, summed over
//! whichever legs a chunk appears in. A chunk missing from a leg
//! contributes nothing from that leg and its rank is reported as `None`
//! ("omit or null a leg the chunk didn't appear in / mode didn't run",
//! architecture.md §12).

use std::collections::HashMap;

/// The fixed RRF constant (architecture.md §12-13; three independent
/// reference implementations and current literature converge on 60).
pub const RRF_K: f64 = 60.0;

/// `1 / (RRF_K + rank)`. `rank` is 1-based (the top hit in a leg has
/// `rank == 1`).
pub fn rrf_term(rank: u32) -> f64 {
    1.0 / (RRF_K + rank as f64)
}

/// The per-leg ranks that produced a chunk's fused score, exactly as
/// returned in the `ask` response's `ranks` object.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FusedRanks {
    pub lexical: Option<u32>,
    pub semantic: Option<u32>,
}

/// Fuses two independently-ranked `(chunk_id, rank)` leg outputs (each
/// 1-based, best match at rank 1) into a per-chunk RRF score plus the
/// per-leg ranks that produced it. Either leg may be empty (a leg that did
/// not run for the current `--mode`, or one that matched nothing).
pub fn fuse(
    lexical: &[(String, u32)],
    semantic: &[(String, u32)],
) -> HashMap<String, (f64, FusedRanks)> {
    let mut out: HashMap<String, (f64, FusedRanks)> = HashMap::new();
    for (id, leg_rank) in lexical {
        let entry = out.entry(id.clone()).or_default();
        entry.0 += rrf_term(*leg_rank);
        entry.1.lexical = Some(*leg_rank);
    }
    for (id, leg_rank) in semantic {
        let entry = out.entry(id.clone()).or_default();
        entry.0 += rrf_term(*leg_rank);
        entry.1.semantic = Some(*leg_rank);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_term_matches_the_documented_formula() {
        // Hand computation: 1/(60+1) and 1/(60+60).
        assert!((rrf_term(1) - 1.0 / 61.0).abs() < 1e-15);
        assert!((rrf_term(60) - 1.0 / 120.0).abs() < 1e-15);
    }

    #[test]
    fn fuse_sums_both_legs_for_a_chunk_present_in_both() {
        // Hand computation: chunk "a" is lexical rank 2, semantic rank 3.
        // score = 1/(60+2) + 1/(60+3) = 1/62 + 1/63
        //       = 0.0161290322580645... + 0.0158730158730158...
        //       = 0.0320020481310804...
        let lexical = vec![("a".to_string(), 2)];
        let semantic = vec![("a".to_string(), 3)];
        let fused = fuse(&lexical, &semantic);
        let (score, ranks) = fused.get("a").expect("chunk a present");
        assert!(
            (score - 0.032_002_048_131_080_4).abs() < 1e-12,
            "got {score}"
        );
        assert_eq!(ranks.lexical, Some(2));
        assert_eq!(ranks.semantic, Some(3));
    }

    #[test]
    fn fuse_leaves_the_absent_legs_rank_as_none() {
        // Hand computation: chunk "b" appears only in the lexical leg at
        // rank 1: score = 1/(60+1) = 1/61 = 0.0163934426229508...
        let lexical = vec![("b".to_string(), 1)];
        let semantic: Vec<(String, u32)> = vec![];
        let fused = fuse(&lexical, &semantic);
        let (score, ranks) = fused.get("b").expect("chunk b present");
        assert!(
            (score - 0.016_393_442_622_950_8).abs() < 1e-12,
            "got {score}"
        );
        assert_eq!(ranks.lexical, Some(1));
        assert_eq!(ranks.semantic, None);
    }

    #[test]
    fn fuse_scores_each_chunk_independently() {
        // Hand computation: both "x" (semantic-only, missing from lexical)
        // and "y" (lexical-only, missing from semantic) score
        // 1/(60+1) = 1/61 = 0.0163934426229508... at rank 1 in their
        // respective single leg.
        let lexical = vec![("y".to_string(), 1)];
        let semantic = vec![("x".to_string(), 1)];
        let fused = fuse(&lexical, &semantic);
        assert!((fused.get("x").unwrap().0 - 1.0 / 61.0).abs() < 1e-15);
        assert!((fused.get("y").unwrap().0 - 1.0 / 61.0).abs() < 1e-15);
        assert_eq!(fused.get("x").unwrap().1.lexical, None);
        assert_eq!(fused.get("x").unwrap().1.semantic, Some(1));
        assert_eq!(fused.get("y").unwrap().1.lexical, Some(1));
        assert_eq!(fused.get("y").unwrap().1.semantic, None);
    }

    #[test]
    fn fuse_a_low_single_leg_rank_can_lose_to_a_hit_in_both_legs() {
        // Hand computation:
        //   chunk "solo": semantic rank 1 only -> 1/61 = 0.0163934426229508...
        //   chunk "both": lexical rank 5, semantic rank 5
        //     -> 1/(60+5) + 1/(60+5) = 2/65 = 0.0307692307692307...
        // "both" must outscore "solo" even though neither of its
        // individual ranks is as good as "solo"'s rank 1.
        let lexical = vec![("both".to_string(), 5)];
        let semantic = vec![("solo".to_string(), 1), ("both".to_string(), 5)];
        let fused = fuse(&lexical, &semantic);
        let solo_score = fused.get("solo").unwrap().0;
        let both_score = fused.get("both").unwrap().0;
        assert!((solo_score - 1.0 / 61.0).abs() < 1e-15);
        assert!((both_score - 2.0 / 65.0).abs() < 1e-15);
        assert!(both_score > solo_score);
    }

    #[test]
    fn fuse_of_two_empty_legs_is_empty() {
        let fused = fuse(&[], &[]);
        assert!(fused.is_empty());
    }
}

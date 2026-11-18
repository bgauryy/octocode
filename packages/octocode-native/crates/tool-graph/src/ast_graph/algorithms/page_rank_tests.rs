use super::{page_rank, rank_tie_class};

fn unit(edges: &[(u32, u32)], len: usize) -> Vec<Vec<(u32, f64)>> {
    let mut out = vec![Vec::new(); len];
    for (src, dst) in edges {
        out[*src as usize].push((*dst, 1.0));
    }
    out
}

/// Members share all rank, a non-member none, whatever the shape:
/// a dangling member gives its rank back to every member.
#[test]
fn member_ranks_sum_to_one_and_others_rank_zero() {
    let members = [0, 1, 2, 4];
    let out = unit(&[(0, 1), (1, 2), (0, 2), (4, 4)], 5);
    let rank = page_rank(5, &members, &out);
    let sum = members.iter().map(|m| rank[*m as usize]).sum::<f64>();
    assert!((sum - 1.0).abs() < 1e-9, "{rank:?}");
    assert_eq!(rank[3], 0.0);
    assert!(rank[2] > rank[1] && rank[1] > rank[0], "{rank:?}");
}

/// A heavier edge carries more rank: two targets of one source, equal
/// by degree, rank by their weights.
#[test]
fn weights_split_rank_in_proportion() {
    let members = [0, 1, 2];
    let out = vec![vec![(1, 3.0), (2, 1.0)], Vec::new(), Vec::new()];
    let rank = page_rank(3, &members, &out);
    assert!(rank[1] > rank[2], "{rank:?}");
    let unweighted = page_rank(3, &members, &unit(&[(0, 1), (0, 2)], 3));
    assert_eq!(
        rank_tie_class(unweighted[1], 3),
        rank_tie_class(unweighted[2], 3)
    );
}

/// Scaling every weight by one factor changes nothing: weights are
/// relative strengths, not probabilities.
#[test]
fn scaling_all_weights_keeps_the_ranks() {
    let members = [0, 1, 2, 3];
    let edges = [(0, 1, 2.0), (1, 2, 1.0), (2, 0, 5.0), (3, 2, 1.0)];
    let mut once = vec![Vec::new(); 4];
    let mut tenfold = vec![Vec::new(); 4];
    for (src, dst, weight) in edges {
        once[src as usize].push((dst, weight));
        tenfold[src as usize].push((dst, weight * 10.0));
    }
    let (a, b) = (
        page_rank(4, &members, &once),
        page_rank(4, &members, &tenfold),
    );
    for member in members {
        let at = member as usize;
        assert_eq!(rank_tie_class(a[at], 4), rank_tie_class(b[at], 4));
    }
}

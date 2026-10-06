//! The unified patch a preview shows: a bounded line diff with context.

/// Unchanged lines shown around each hunk of the preview patch.
const PATCH_CONTEXT: usize = 3;
/// Edit-distance budget for the line diff. Past it the changed region is shown
/// as one replaced block: still a correct patch, just not minimal.
const PATCH_MAX_EDITS: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum LineOp {
    Equal,
    Delete,
    Insert,
}

/// Myers shortest edit script over lines. `None` when the script needs more
/// than `max_edits` insertions plus deletions. Memory is O(D²) in the edit
/// count, not O(N·M) in the file size.
pub(super) fn line_diff(old: &[&str], new: &[&str], max_edits: usize) -> Option<Vec<LineOp>> {
    let (n, m) = (old.len() as isize, new.len() as isize);
    let limit = (old.len() + new.len()).min(max_edits) as isize;
    let offset = limit + 1;
    let mut v = vec![0isize; (2 * offset + 1) as usize];
    // trace[d] holds v[-(d+1)..=(d+1)] as it was before step d.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut found = false;
    'search: for d in 0..=limit {
        trace.push(v[(offset - d - 1) as usize..=(offset + d + 1) as usize].to_vec());
        let mut k = -d;
        while k <= d {
            let at = |k: isize| (offset + k) as usize;
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && old[x as usize] == new[y as usize] {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x >= n && y >= m {
                found = true;
                break 'search;
            }
            k += 2;
        }
    }
    if !found {
        return None;
    }
    let mut ops = Vec::with_capacity(old.len() + new.len());
    let (mut x, mut y) = (n, m);
    for (d, snapshot) in trace.iter().enumerate().rev() {
        let d = d as isize;
        let get = |k: isize| snapshot[(k + d + 1) as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && get(k - 1) < get(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = get(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            ops.push(LineOp::Equal);
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            ops.push(if x == prev_x {
                LineOp::Insert
            } else {
                LineOp::Delete
            });
            x = prev_x;
            y = prev_y;
        }
    }
    ops.reverse();
    Some(ops)
}

pub(super) fn create_unified_patch(path: &str, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }
    // Split preserving line endings so the preview reflects CRLF and missing
    // final newlines faithfully; `str::lines()` would drop that information and
    // produce hunk bodies that disagree with the bytes on disk. This is a
    // display-only preview; the actual apply is a byte splice elsewhere.
    let old = before.split_inclusive('\n').collect::<Vec<_>>();
    let new = after.split_inclusive('\n').collect::<Vec<_>>();
    let mut prefix = 0usize;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < old.len().saturating_sub(prefix)
        && suffix < new.len().saturating_sub(prefix)
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let old_mid = &old[prefix..old.len() - suffix];
    let new_mid = &new[prefix..new.len() - suffix];
    let middle = line_diff(old_mid, new_mid, PATCH_MAX_EDITS).unwrap_or_else(|| {
        let mut block = vec![LineOp::Delete; old_mid.len()];
        block.extend(std::iter::repeat_n(LineOp::Insert, new_mid.len()));
        block
    });
    // Whole-file script, each op with the old/new line index it consumes.
    let mut ops = Vec::with_capacity(prefix + middle.len() + suffix);
    let (mut oi, mut ni) = (0usize, 0usize);
    for op in std::iter::repeat_n(LineOp::Equal, prefix)
        .chain(middle)
        .chain(std::iter::repeat_n(LineOp::Equal, suffix))
    {
        ops.push((op, oi, ni));
        match op {
            LineOp::Equal => {
                oi += 1;
                ni += 1;
            }
            LineOp::Delete => oi += 1,
            LineOp::Insert => ni += 1,
        }
    }
    // Group changes whose unchanged gap fits inside two contexts into one hunk.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (index, (op, _, _)) in ops.iter().enumerate() {
        if *op == LineOp::Equal {
            continue;
        }
        match groups.last_mut() {
            Some(group) if index - group.1 <= 2 * PATCH_CONTEXT + 1 => group.1 = index,
            _ => groups.push((index, index)),
        }
    }
    let mut patch = String::new();
    patch.push_str(&format!("--- a/{path}\n"));
    patch.push_str(&format!("+++ b/{path}\n"));
    for (first, last) in groups {
        let lo = first.saturating_sub(PATCH_CONTEXT);
        let hi = (last + PATCH_CONTEXT).min(ops.len() - 1);
        let hunk = &ops[lo..=hi];
        let old_count = hunk.iter().filter(|(op, ..)| *op != LineOp::Insert).count();
        let new_count = hunk.iter().filter(|(op, ..)| *op != LineOp::Delete).count();
        // An empty side names the line before the hunk (unified-diff rule).
        let start = |index: usize, count: usize| if count == 0 { index } else { index + 1 };
        patch.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            start(hunk[0].1, old_count),
            old_count,
            start(hunk[0].2, new_count),
            new_count
        ));
        for &(op, oi, ni) in hunk {
            let (marker, line) = match op {
                LineOp::Equal => (' ', old[oi]),
                LineOp::Delete => ('-', old[oi]),
                LineOp::Insert => ('+', new[ni]),
            };
            patch.push(marker);
            patch.push_str(line);
            if !line.ends_with('\n') {
                // A line without a trailing newline (final line of a no-EOF-newline
                // file) still needs to terminate the diff row it lives on.
                patch.push('\n');
                patch.push_str("\\ No newline at end of file\n");
            }
        }
    }
    patch
}

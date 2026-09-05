use crate::tokenizer::Line;
use imara_diff::{Algorithm, Diff, Hunk, InternedInput};
use std::cmp::min;

fn diff_lines(base: &[Line], modified: &[Line]) -> Vec<Hunk> {
    // Histogram, not Myers: CHANGES.txt repeats the same prefix 1000x,
    // Myers goes quadratic there.
    // Using postprocess to keep the hunks line-aligned
    // so adjacency checks below work on whole lines.
    let mut input = InternedInput::default();
    input.update_before(base.iter().map(|l| l.content));
    input.update_after(modified.iter().map(|l| l.content));
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    diff.hunks().collect()
}

pub enum DiffRegion<'a> {
    Unchanged {
        lines: &'a [Line<'a>],
    },
    LocalOnly {
        base: &'a [Line<'a>],
        local: &'a [Line<'a>],
    },
    RemoteOnly {
        base: &'a [Line<'a>],
        remote: &'a [Line<'a>],
    },
    BothSame {
        base: &'a [Line<'a>],
        resolved: &'a [Line<'a>],
    },
    Conflict {
        base: &'a [Line<'a>],
        local: &'a [Line<'a>],
        remote: &'a [Line<'a>],
    },
}

/// Split base/local/remote into regions. Ports svn_diff_diff3_2 logic:
/// two 2-way diffs, then walk hunks — adjacent (touching, not overlapping)
/// stays separate so CHANGES.txt entries merge without conflict.
pub fn reconcile<'a>(
    base: &'a [Line<'a>],
    local: &'a [Line<'a>],
    remote: &'a [Line<'a>],
) -> Vec<DiffRegion<'a>> {
    let local_hunks = diff_lines(base, local);
    let remote_hunks = diff_lines(base, remote);

    let mut regions = Vec::new();
    let mut base_pos: usize = 0;
    let mut li = 0;
    let mut ri = 0;

    while li < local_hunks.len() || ri < remote_hunks.len() {
        let lh = local_hunks.get(li);
        let rh = remote_hunks.get(ri);

        let l_start = lh.map(|h| h.before.start as usize).unwrap_or(usize::MAX);
        let r_start = rh.map(|h| h.before.start as usize).unwrap_or(usize::MAX);

        // Emit stable lines before the next hunk. cf. svn_diff_diff3_2 in
        // subversion/libsvn_diff/diff3.c — it walks both hunk lists the same way.
        let unchanged_end = min(l_start, r_start);
        if unchanged_end > base_pos {
            regions.push(DiffRegion::Unchanged {
                lines: &base[base_pos..unchanged_end],
            })
        }
        base_pos = unchanged_end;

        match (lh, rh) {
            (Some(l), None) => {
                let br = l.before.start as usize..l.before.end as usize;
                let lr = l.after.start as usize..l.after.end as usize;
                regions.push(DiffRegion::LocalOnly {
                    base: &base[br],
                    local: &local[lr],
                });
                base_pos = l.before.end as usize;
                li += 1;
            }
            (None, Some(r)) => {
                let br = r.before.start as usize..r.before.end as usize;
                let rr = r.after.start as usize..r.after.end as usize;
                regions.push(DiffRegion::RemoteOnly {
                    base: &base[br],
                    remote: &remote[rr],
                });
                base_pos = r.before.end as usize;
                ri += 1;
            }
            (Some(l), Some(r)) => {
                // Strict > : touching hunks (end==start) are separate.
                // git's xdiff merges them into one conflict — that's the bug we're fixing.
                let overlapping = l.before.end > r.before.start && r.before.end > l.before.start;
                let same_modification = l.before == r.before && !l.before.is_empty();

                if overlapping || same_modification {
                    let base_start = min(l.before.start, r.before.start) as usize;
                    let base_end = std::cmp::max(l.before.end, r.before.end) as usize;
                    let lr = l.after.start as usize..l.after.end as usize;
                    let rr = r.after.start as usize..r.after.end as usize;
                    let local_slice = &local[lr];
                    let remote_slice = &remote[rr];
                    let same = local_slice.len() == remote_slice.len()
                        && local_slice
                            .iter()
                            .zip(remote_slice.iter())
                            .all(|(a, b)| a.content == b.content);

                    if same {
                        regions.push(DiffRegion::BothSame {
                            base: &base[base_start..base_end],
                            resolved: local_slice,
                        });
                    } else {
                        regions.push(DiffRegion::Conflict {
                            base: &base[base_start..base_end],
                            local: local_slice,
                            remote: remote_slice,
                        });
                    }
                    base_pos = base_end;
                    li += 1;
                    ri += 1;
                } else if l.before == r.before && l.before.is_empty() {
                    let lr = l.after.start as usize..l.after.end as usize;
                    let rr = r.after.start as usize..r.after.end as usize;
                    let local_slice = &local[lr];
                    let remote_slice = &remote[rr];
                    let same = local_slice.len() == remote_slice.len()
                        && local_slice
                            .iter()
                            .zip(remote_slice.iter())
                            .all(|(a, b)| a.content == b.content);
                    if same {
                        regions.push(DiffRegion::BothSame {
                            base: &base[l.before.start as usize..l.before.end as usize],
                            resolved: local_slice,
                        });
                    } else {
                        // Same spot, different text: svn keeps both, in order.
                        // git calls this a conflict; for CHANGES.txt both entries are wanted.
                        let br = l.before.start as usize..l.before.end as usize;
                        regions.push(DiffRegion::LocalOnly {
                            base: &base[br.clone()],
                            local: local_slice,
                        });
                        regions.push(DiffRegion::RemoteOnly {
                            base: &base[br],
                            remote: remote_slice,
                        });
                    }
                    li += 1;
                    ri += 1;
                } else if l.before.start <= r.before.start {
                    let br = l.before.start as usize..l.before.end as usize;
                    let lr = l.after.start as usize..l.after.end as usize;
                    regions.push(DiffRegion::LocalOnly {
                        base: &base[br],
                        local: &local[lr],
                    });
                    base_pos = l.before.end as usize;
                    li += 1;
                } else {
                    let br = r.before.start as usize..r.before.end as usize;
                    let rr = r.after.start as usize..r.after.end as usize;
                    regions.push(DiffRegion::RemoteOnly {
                        base: &base[br],
                        remote: &remote[rr],
                    });
                    base_pos = r.before.end as usize;
                    ri += 1;
                }
            }
            (None, None) => break,
        }
    }

    if base_pos < base.len() {
        regions.push(DiffRegion::Unchanged {
            lines: &base[base_pos..],
        });
    }

    debug_assert!(base_pos <= base.len());
    regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::tokenize;

    fn tok(s: &str) -> Vec<Line<'_>> {
        tokenize(s.as_bytes()).unwrap()
    }

    #[test]
    fn no_overlap_clean_merge() {
        // LOCAL adds at start, REMOTE adds at end
        let base = tok("Aa\nBb\nCc\n");
        let local = tok("Xx\nAa\nBb\nCc\n");
        let remote = tok("Aa\nBb\nCc\nYy\n");

        let regions = reconcile(&base, &local, &remote);

        assert!(!regions
            .iter()
            .any(|r| matches!(r, DiffRegion::Conflict { .. })));
    }

    #[test]
    fn adjacent_changes_clean_merge() {
        // THE KEY TEST — Git conflicts, we resolve
        let base = tok("foo\nbar\nbaz\n");
        let local = tok("foo\nnew_bar\nbaz\n");
        let remote = tok("foo\nbar\nnew_baz\n");

        let regions = reconcile(&base, &local, &remote);

        // Must NOT be a conflict
        assert!(!regions
            .iter()
            .any(|r| matches!(r, DiffRegion::Conflict { .. })));
    }

    #[test]
    fn genuine_conflict() {
        // Both changed same line differently
        let base = tok("Aa\nBb\nCc\n");
        let local = tok("Aa\nXx\nCc\n");
        let remote = tok("Aa\nYy\nCc\n");

        let regions = reconcile(&base, &local, &remote);

        assert!(regions
            .iter()
            .any(|r| matches!(r, DiffRegion::Conflict { .. })));
    }

    #[test]
    fn idempotent_both_same() {
        // Both sides made same change — take once
        let base = tok("Aa\nold\nBb\n");
        let local = tok("Aa\nnew\nBb\n");
        let remote = tok("Aa\nnew\nBb\n");

        let regions = reconcile(&base, &local, &remote);

        assert!(!regions
            .iter()
            .any(|r| matches!(r, DiffRegion::Conflict { .. })));
        assert!(regions
            .iter()
            .any(|r| matches!(r, DiffRegion::BothSame { .. })));
    }

    // Golden files live in output.rs tests; don't duplicate them here.

    #[test]
    fn same_insertion_point_identical_content() {
        // Both sides insert the same line — should deduplicate, not emit twice
        let base = tok("Aa\nCc\n");
        let local = tok("Aa\nBb\nCc\n");
        let remote = tok("Aa\nBb\nCc\n");

        let regions = reconcile(&base, &local, &remote);

        assert!(!regions
            .iter()
            .any(|r| matches!(r, DiffRegion::Conflict { .. })));
        assert!(regions
            .iter()
            .any(|r| matches!(r, DiffRegion::BothSame { .. })));
    }

    #[test]
    fn same_insertion_point_different_content() {
        // Both sides insert different content at same point — both kept (SVN behavior)
        let base = tok("Aa\nCc\n");
        let local = tok("Aa\nXx\nCc\n");
        let remote = tok("Aa\nYy\nCc\n");

        let regions = reconcile(&base, &local, &remote);

        assert!(!regions
            .iter()
            .any(|r| matches!(r, DiffRegion::Conflict { .. })));
    }
}

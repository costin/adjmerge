use crate::diff3::DiffRegion;
use crate::tokenizer::{Eol, Line};
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStyle {
    Diff3,
    Zdiff3,
}

pub struct Labels<'a> {
    pub local: &'a str,
    pub base: &'a str,
    pub remote: &'a str,
}

impl Default for Labels<'_> {
    fn default() -> Self {
        Labels {
            local: "LOCAL",
            base: "BASE",
            remote: "REMOTE",
        }
    }
}

fn write_line<W: Write>(line: &Line, out: &mut W) -> io::Result<()> {
    out.write_all(line.content.as_bytes())?;
    out.write_all(match line.eol {
        Eol::Lf => b"\n",
        Eol::CrLf => b"\r\n",
        Eol::Cr => b"\r",
        Eol::None => b"",
    })
}

fn write_lines<W: Write>(lines: &[Line], out: &mut W) -> io::Result<()> {
    for line in lines {
        write_line(line, out)?;
    }
    Ok(())
}

/// Count how many lines at the start of both slices have the same content.
fn common_prefix_len(a: &[Line], b: &[Line]) -> usize {
    let mut count = 0;
    let limit = a.len().min(b.len());
    while count < limit && a[count].content == b[count].content {
        count += 1;
    }
    count
}

/// Count how many lines at the end of both slices have the same content,
/// not overlapping with the already-matched prefix.
fn common_suffix_len(a: &[Line], b: &[Line], prefix_len: usize) -> usize {
    let mut count = 0;
    let a_remaining = a.len() - prefix_len;
    let b_remaining = b.len() - prefix_len;
    let limit = a_remaining.min(b_remaining);
    while count < limit && a[a.len() - 1 - count].content == b[b.len() - 1 - count].content {
        count += 1;
    }
    count
}

fn write_conflict<W: Write>(
    out: &mut W,
    labels: &Labels<'_>,
    local: &[Line],
    base: &[Line],
    remote: &[Line],
) -> io::Result<()> {
    writeln!(out, "<<<<<<< {}", labels.local)?;
    write_lines(local, out)?;
    writeln!(out, "||||||| {}", labels.base)?;
    write_lines(base, out)?;
    writeln!(out, "=======")?;
    write_lines(remote, out)?;
    writeln!(out, ">>>>>>> {}", labels.remote)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    Clean,
    AutoResolved,
    Conflict,
}

/// Write merged output to any impl Write.
pub fn write_merged<W: Write>(
    regions: &[DiffRegion<'_>],
    out: &mut W,
    style: ConflictStyle,
    labels: &Labels<'_>,
) -> io::Result<MergeOutcome> {
    let mut had_conflict = false;
    let mut had_auto = false;

    for region in regions {
        match region {
            DiffRegion::Unchanged { lines } => {
                write_lines(lines, out)?;
            }
            DiffRegion::LocalOnly { local, .. } => {
                had_auto = true;
                write_lines(local, out)?;
            }
            DiffRegion::RemoteOnly { remote, .. } => {
                had_auto = true;
                write_lines(remote, out)?;
            }
            DiffRegion::BothSame { resolved, .. } => {
                write_lines(resolved, out)?;
            }
            DiffRegion::Conflict {
                base,
                local,
                remote,
            } => {
                had_conflict = true;
                match style {
                    ConflictStyle::Diff3 => {
                        write_conflict(out, labels, local, base, remote)?;
                    }
                    ConflictStyle::Zdiff3 => {
                        let prefix = common_prefix_len(local, remote);
                        let suffix = common_suffix_len(local, remote, prefix);
                        // Base trim has to agree with *both* sides, not just local.
                        // Trimming vs local only drops base lines that were never
                        // moved outside.
                        let base_prefix = common_prefix_len(base, local)
                            .min(common_prefix_len(base, remote))
                            .min(prefix);
                        let base_suffix = common_suffix_len(base, local, base_prefix)
                            .min(common_suffix_len(base, remote, base_prefix))
                            .min(suffix);

                        write_lines(&local[..prefix], out)?;
                        write_conflict(
                            out,
                            labels,
                            &local[prefix..local.len() - suffix],
                            &base[base_prefix..base.len().saturating_sub(base_suffix)],
                            &remote[prefix..remote.len() - suffix],
                        )?;
                        write_lines(&local[local.len() - suffix..], out)?;
                    }
                }
            }
        }
    }

    Ok(if had_conflict {
        MergeOutcome::Conflict
    } else if had_auto {
        MergeOutcome::AutoResolved
    } else {
        MergeOutcome::Clean
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff3::reconcile;
    use crate::tokenizer::tokenize;

    fn merge_to_string(
        base: &str,
        local: &str,
        remote: &str,
        style: ConflictStyle,
    ) -> (String, MergeOutcome) {
        let base_lines = tokenize(base.as_bytes()).unwrap();
        let local_lines = tokenize(local.as_bytes()).unwrap();
        let remote_lines = tokenize(remote.as_bytes()).unwrap();

        let regions = reconcile(&base_lines, &local_lines, &remote_lines);

        let mut buf = Vec::new();
        let outcome = write_merged(&regions, &mut buf, style, &Labels::default()).unwrap();

        (String::from_utf8(buf).unwrap(), outcome)
    }

    #[test]
    fn clean_merge_auto_resolved() {
        let (out, outcome) = merge_to_string(
            "Aa\nBb\nCc\n",
            "Xx\nAa\nBb\nCc\n",
            "Aa\nBb\nCc\nYy\n",
            ConflictStyle::Diff3,
        );
        assert_eq!(outcome, MergeOutcome::AutoResolved);
        assert_eq!(out, "Xx\nAa\nBb\nCc\nYy\n");
    }

    #[test]
    fn adjacent_merge_auto_resolved() {
        let (out, outcome) = merge_to_string(
            "foo\nbar\nbaz\n",
            "foo\nnew_bar\nbaz\n",
            "foo\nbar\nnew_baz\n",
            ConflictStyle::Diff3,
        );
        assert_eq!(outcome, MergeOutcome::AutoResolved);
        assert_eq!(out, "foo\nnew_bar\nnew_baz\n");
    }

    #[test]
    fn identical_files_clean() {
        let (out, outcome) = merge_to_string(
            "Aa\nBb\nCc\n",
            "Aa\nBb\nCc\n",
            "Aa\nBb\nCc\n",
            ConflictStyle::Diff3,
        );
        assert_eq!(outcome, MergeOutcome::Clean);
        assert_eq!(out, "Aa\nBb\nCc\n");
    }

    #[test]
    fn conflict_markers_diff3() {
        let (out, outcome) = merge_to_string(
            "Aa\nBb\nCc\n",
            "Aa\nXx\nCc\n",
            "Aa\nYy\nCc\n",
            ConflictStyle::Diff3,
        );
        assert_eq!(outcome, MergeOutcome::Conflict);
        assert!(out.contains("<<<<<<< LOCAL"));
        assert!(out.contains("||||||| BASE"));
        assert!(out.contains("======="));
        assert!(out.contains(">>>>>>> REMOTE"));
    }

    #[test]
    fn conflict_markers_zdiff3() {
        let (out, outcome) = merge_to_string(
            "Aa\nBb\nCc\n",
            "Aa\nXx\nCc\n",
            "Aa\nYy\nCc\n",
            ConflictStyle::Zdiff3,
        );
        assert_eq!(outcome, MergeOutcome::Conflict);
        assert!(out.contains("<<<<<<< LOCAL"));
        let marker_start = out.find("<<<<<<< LOCAL").unwrap();
        let marker_end = out.find(">>>>>>> REMOTE").unwrap();
        let before = &out[..marker_start];
        let after = &out[marker_end..];
        assert!(before.contains("Aa\n"));
        assert!(after.contains("Cc\n"));
    }

    #[test]
    fn zdiff3_asymmetric_base_kept() {
        // zdiff3-only: base matches local (A B) but not remote (Q...).
        let base_lines = tokenize(b"A\nB\nC\n").unwrap();
        let local_lines = tokenize(b"A\nB\nX\n").unwrap();
        let remote_lines = tokenize(b"Q\nB\nX\n").unwrap();
        let regions = vec![DiffRegion::Conflict {
            base: &base_lines,
            local: &local_lines,
            remote: &remote_lines,
        }];

        let mut buf = Vec::new();
        let outcome = write_merged(
            &regions,
            &mut buf,
            ConflictStyle::Zdiff3,
            &Labels::default(),
        )
        .unwrap();
        assert_eq!(outcome, MergeOutcome::Conflict);
        let out = String::from_utf8(buf).unwrap();

        // shared suffix [B X] moved outside markers
        assert!(out.ends_with("B\nX\n"), "suffix not moved out:\n{}", out);
        // base section must keep full context
        let base_start = out.find("||||||| BASE").unwrap();
        let div = out.find("=======").unwrap();
        let base_section = &out[base_start..div];
        assert!(
            base_section.contains("A\n") && base_section.contains("B\n"),
            "base trimmed too far (old bug):\n{}",
            out
        );
    }

    #[test]
    fn both_same_clean() {
        let (out, outcome) = merge_to_string(
            "Aa\nold\nBb\n",
            "Aa\nnew\nBb\n",
            "Aa\nnew\nBb\n",
            ConflictStyle::Diff3,
        );
        assert_eq!(outcome, MergeOutcome::Clean);
        assert_eq!(out, "Aa\nnew\nBb\n");
    }

    #[test]
    fn adjacent1_golden_test() {
        let base = std::fs::read_to_string("tests/cases/adjacent1/base").unwrap();
        let local = std::fs::read_to_string("tests/cases/adjacent1/local").unwrap();
        let remote = std::fs::read_to_string("tests/cases/adjacent1/remote").unwrap();
        let expected = std::fs::read_to_string("tests/cases/adjacent1/expected_merged").unwrap();

        let base_lines = tokenize(base.as_bytes()).unwrap();
        let local_lines = tokenize(local.as_bytes()).unwrap();
        let remote_lines = tokenize(remote.as_bytes()).unwrap();

        let regions = reconcile(&base_lines, &local_lines, &remote_lines);

        let mut buf = Vec::new();
        let outcome =
            write_merged(&regions, &mut buf, ConflictStyle::Diff3, &Labels::default()).unwrap();

        assert_eq!(outcome, MergeOutcome::AutoResolved);
        assert_eq!(String::from_utf8(buf).unwrap(), expected);
    }

    #[test]
    fn lucene_golden_test() {
        // Real conflict from apache/lucene PR #16378
        let base = std::fs::read_to_string("tests/cases/lucene/base").unwrap();
        let local = std::fs::read_to_string("tests/cases/lucene/local").unwrap();
        let remote = std::fs::read_to_string("tests/cases/lucene/remote").unwrap();
        let expected = std::fs::read_to_string("tests/cases/lucene/expected_merged").unwrap();

        let base_lines = tokenize(base.as_bytes()).unwrap();
        let local_lines = tokenize(local.as_bytes()).unwrap();
        let remote_lines = tokenize(remote.as_bytes()).unwrap();

        let regions = reconcile(&base_lines, &local_lines, &remote_lines);

        let mut buf = Vec::new();
        let outcome =
            write_merged(&regions, &mut buf, ConflictStyle::Diff3, &Labels::default()).unwrap();

        assert_eq!(outcome, MergeOutcome::AutoResolved);
        assert_eq!(String::from_utf8(buf).unwrap(), expected);
    }
}

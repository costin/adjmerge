# Test Cases

Each subdirectory contains: base, local, remote, expected_merged files.

## Origin

zig, splish, and dig cases are derived from Apache Subversion's `diff-diff3-test.c`
([source](https://github.com/apache/subversion/blob/trunk/subversion/tests/libsvn_diff/diff-diff3-test.c)).
Names correspond to sub-cases in `test_three_way_merge_no_overlap` (zig),
`test_three_way_merge_with_overlap` (splish), and `test_three_way_merge_with_conflict` (dig).

adjacent1 is a synthetic test based on Lucene's `SingletonSortedNumericDocValues`
(PR [#16295](https://github.com/apache/lucene/pull/16295)).

lucene is a real conflict from Lucene's CHANGES.txt
(PR [#16378](https://github.com/apache/lucene/pull/16378)) where conflict markers
were committed to main for ~4 days.

## Index

| Name | What it validates |
|------|-------------------|
| adjacent1 | Adjacent line changes in Java class — Git conflicts, adjmerge resolves |
| lucene | Real CHANGES.txt conflict — two independent entries after same anchor |
| dig4 | Both sides add different content at end (no trailing newline) — conflict |
| splish2 | Repeated tokens, 3 occurrences |
| zig1 | Insert at start + insert at end — clean merge |

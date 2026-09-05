pub mod diff3;
pub mod error;
pub mod output;
pub mod tokenizer;

use error::MergeError;
use output::{ConflictStyle, Labels, MergeOutcome};
use std::io::Write;

/// Merge base/local/remote bytes, write result.
/// Used by the git driver and by tests via merge_to_string.
pub fn merge<W: Write>(
    base: &[u8],
    local: &[u8],
    remote: &[u8],
    out: &mut W,
    style: ConflictStyle,
    labels: &Labels<'_>,
) -> Result<MergeOutcome, MergeError> {
    let base_lines = tokenizer::tokenize(base)?;
    let local_lines = tokenizer::tokenize(local)?;
    let remote_lines = tokenizer::tokenize(remote)?;

    let regions = diff3::reconcile(&base_lines, &local_lines, &remote_lines);

    output::write_merged(&regions, out, style, labels).map_err(MergeError::from)
}

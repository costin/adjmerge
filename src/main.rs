use adjmerge::output::{ConflictStyle, Labels, MergeOutcome};
use std::fs;

fn run() -> Result<(MergeOutcome, bool), adjmerge::error::MergeError> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains("--version") || args.contains("-V") {
        println!("adjmerge {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    if args.contains("--help") || args.contains("-h") {
        println!(
            "usage: adjmerge [--auto] [--style diff3|zdiff3] <base> <local> <remote> <output>"
        );
        std::process::exit(0);
    }

    let auto = args.contains("--auto");

    let style_str: Option<String> = match args.opt_value_from_str("--style") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("adjmerge: --style expects diff3 or zdiff3");
            std::process::exit(2);
        }
    };

    let style = match style_str.as_deref() {
        Some("zdiff3") => ConflictStyle::Zdiff3,
        Some("diff3") | None => ConflictStyle::Diff3,
        Some(other) => {
            eprintln!("adjmerge: unknown style '{}' (use diff3 or zdiff3)", other);
            std::process::exit(2);
        }
    };

    let remaining = args.finish();
    let mut positional = Vec::with_capacity(remaining.len());
    for s in remaining {
        match s.into_string() {
            Ok(s) => positional.push(s),
            Err(_) => {
                eprintln!("adjmerge: arguments must be utf-8 paths");
                std::process::exit(2);
            }
        }
    }

    if positional.len() != 4 {
        eprintln!(
            "usage: adjmerge [--auto] [--style diff3|zdiff3] <base> <local> <remote> <output>"
        );
        std::process::exit(2);
    }

    let base = fs::read(&positional[0])?;
    let local = fs::read(&positional[1])?;
    let remote = fs::read(&positional[2])?;

    // %O %A %B are temp paths from git, useless as conflict labels,
    // but that's all the driver protocol gives us.
    let labels = Labels {
        local: &positional[1],
        base: &positional[0],
        remote: &positional[2],
    };

    // git overwrites %A in place; write temp in same dir then rename,
    // so a crash mid-merge doesn't leave a truncated file.
    let out_path = std::path::PathBuf::from(&positional[3]);
    // use the pid for the temp name (vs random)
    let parent = out_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    let tmp_name = format!(
        ".{}.tmp.{}",
        out_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "adjmerge".to_string()),
        std::process::id()
    );
    let tmp_path = parent.join(tmp_name);
    let outcome: Result<MergeOutcome, adjmerge::error::MergeError> = (|| {
        let mut tmp = fs::File::create(&tmp_path)?;
        let outcome = adjmerge::merge(&base, &local, &remote, &mut tmp, style, &labels)?;
        drop(tmp);
        fs::rename(&tmp_path, &out_path)?;
        Ok(outcome)
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    Ok((outcome?, auto))
}

fn main() {
    match run() {
        Ok((MergeOutcome::Clean, _)) => std::process::exit(0),
        Ok((MergeOutcome::AutoResolved, true)) => std::process::exit(0),
        Ok((MergeOutcome::AutoResolved, false)) => {
            eprintln!("adjmerge: auto-resolved, review the result");
            std::process::exit(1);
        }
        Ok((MergeOutcome::Conflict, _)) => std::process::exit(1),
        Err(e) => {
            eprintln!("adjmerge: error: {}", e);
            std::process::exit(2);
        }
    }
}

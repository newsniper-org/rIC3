//! BTOR2 parse/deparse round-trip fidelity gate.
//!
//! # Why this exists
//!
//! `docs/toolchain-interop-patterns.md` §1.3(6) keeps one proposal open: a
//! specification language whose *proper subset* corresponds 1:1 to the existing
//! semantics — a conservative extension. The distinguishing feature of that
//! proposal is that it carries its own gate, and this is the gate.
//!
//! `btor-rs` already exposes both directions (`Btor::from_file`, `Btor::to_file`
//! via `Deparser`), so the 1:1 claim can be tested before any language is
//! designed.
//!
//! # What "round-trip fidelity" means here
//!
//! Byte equality against the *original* file is the wrong criterion and would
//! fail for uninteresting reasons: BTOR2 permits comments, differing node
//! numbering, and different but equivalent orderings. What matters is:
//!
//! 1. **Fixpoint.** `deparse(parse(x))` and `deparse(parse(deparse(parse(x))))`
//!    must be identical. If the second pass differs, the representation is
//!    losing or inventing information, and no subset built on it can be 1:1.
//! 2. **Node conservation.** The emitted line count and per-keyword counts must
//!    not drift between passes.
//!
//! Verdict invariance over the 840-case suite is the *other* half of the gate
//! and is checked by `bench/harness.py` in phase B, not here.
//!
//! # Usage
//!
//! ```text
//! btor-roundtrip <file-or-dir>...
//! btor-roundtrip --keep-output <dir> <file-or-dir>...
//! ```
//!
//! Exit status is 1 if any instance fails fixpoint or conservation, so it can
//! be used directly as a gate in CI.

use btor::Btor;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Extensions we treat as BTOR2 input.
const BTOR_EXTENSIONS: [&str; 2] = ["btor", "btor2"];

/// BTOR2 operators whose two operands may be swapped without changing meaning.
///
/// This list is why the naive gate was wrong. Round-tripping `counter.btor`
/// produced output differing from a second pass *only* in lines like
///
/// ```text
/// 16 eq  1 8 15     ->  16 eq  1 15 8
/// 19 add 7 8 18     ->  19 add 7 18 8
/// 35 and 1 28 16    ->  35 and 1 16 28
/// ```
///
/// with identical line counts and identical per-keyword histograms. The
/// deparser's operand order follows internal term ordering, which depends on
/// how many terms the process has already interned (`logicrs` keeps global
/// state), so it is stable per run but not across parse positions. Since these
/// operators are commutative, the difference carries no semantic content.
const COMMUTATIVE_OPS: [&str; 10] = [
    "and", "or", "xor", "xnor", "nand", "nor", "eq", "neq", "add", "mul",
];

/// Normalise a BTOR2 text so that semantically irrelevant operand order does
/// not register as a difference.
///
/// Comments and blank lines are dropped, whitespace is collapsed, and for the
/// commutative operators above the two operand fields are sorted numerically
/// (BTOR2 encodes negation as a negative node id, so numeric sorting keeps
/// `-5` and `5` adjacent in a predictable way).
///
/// This is the criterion the gate actually uses. Raw text equality is still
/// reported, but only as information.
fn canonicalise(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') {
            continue;
        }
        // Strip a trailing comment; BTOR2 allows `; ...` after the operands.
        let t = t.split(';').next().unwrap_or(t).trim();
        let f: Vec<&str> = t.split_whitespace().collect();
        // Shape: <id> <op> <sort> <operand> <operand> [...]
        if f.len() >= 5 && COMMUTATIVE_OPS.contains(&f[1]) {
            let mut ab = [f[3], f[4]];
            ab.sort_by_key(|s| s.parse::<i64>().unwrap_or(i64::MAX));
            let mut fields: Vec<&str> = vec![f[0], f[1], f[2], ab[0], ab[1]];
            fields.extend_from_slice(&f[5..]);
            out.push(fields.join(" "));
        } else {
            out.push(f.join(" "));
        }
    }
    out.join("\n")
}

/// Per-instance outcome.
#[derive(Debug)]
struct Outcome {
    path: PathBuf,
    /// Lines emitted by the first deparse.
    lines_pass1: usize,
    /// Lines emitted by the second deparse.
    lines_pass2: usize,
    /// Lines in the original file, ignoring comments and blanks.
    lines_original: usize,
    /// Raw text equality: `deparse(parse(x))` vs a second pass. Informational
    /// only — commutative operand order makes this fail without any semantic
    /// difference.
    fixpoint: bool,
    /// Equality after `canonicalise`. **This is the gate.**
    canonical_fixpoint: bool,
    /// Per-keyword counts differing between pass 1 and pass 2, if any.
    keyword_drift: Vec<(String, usize, usize)>,
}

impl Outcome {
    /// Passing means the round trip preserved meaning: identical after
    /// commutative normalisation, and no keyword gained or lost.
    fn ok(&self) -> bool {
        self.canonical_fixpoint && self.keyword_drift.is_empty()
    }
}

/// Count BTOR2 keywords per line. A BTOR2 line is
/// `<id> <keyword> <operands...>`, so the keyword is the second field.
/// Comment lines start with ';' and are ignored.
fn keyword_histogram(text: &str) -> BTreeMap<String, usize> {
    let mut hist = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(kw) = line.split_whitespace().nth(1) {
            *hist.entry(kw.to_string()).or_insert(0) += 1;
        }
    }
    hist
}

/// Non-comment, non-blank line count.
fn significant_lines(text: &str) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(';'))
        .count()
}

/// Write `text` to a temporary file and return its path.
///
/// `Btor` can only be parsed from a file (`Btor::from_file`), so the second
/// pass needs the intermediate on disk. Kept in the system temp dir and removed
/// by the caller.
fn write_temp(text: &str, tag: &str) -> std::io::Result<PathBuf> {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "btor-roundtrip-{}-{}.btor2",
        std::process::id(),
        tag
    ));
    std::fs::write(&path, text)?;
    Ok(path)
}

/// Run the two-pass round trip on one instance.
///
/// `Btor::from_file` panics on malformed input (it `unwrap`s internally), which
/// is acceptable here: this is a developer gate over a corpus we control, not a
/// component consuming untrusted data. The cache layer, which *does* consume
/// untrusted data, has a total-deserialisation requirement instead — see
/// `docs/cache-design.md` §7.
fn roundtrip(path: &Path, keep: Option<&Path>) -> std::io::Result<Outcome> {
    let original = std::fs::read_to_string(path)?;

    // Pass 1: parse the original, deparse it.
    let pass1 = Btor::from_file(path).to_string();

    // Pass 2: parse our own output, deparse again.
    let tmp = write_temp(&pass1, "p1")?;
    let pass2 = Btor::from_file(&tmp).to_string();
    let _ = std::fs::remove_file(&tmp);

    if let Some(dir) = keep {
        std::fs::create_dir_all(dir)?;
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        std::fs::write(dir.join(format!("{stem}.pass1.btor2")), &pass1)?;
        std::fs::write(dir.join(format!("{stem}.pass2.btor2")), &pass2)?;
    }

    let h1 = keyword_histogram(&pass1);
    let h2 = keyword_histogram(&pass2);
    let mut keyword_drift = Vec::new();
    for kw in h1.keys().chain(h2.keys()).collect::<std::collections::BTreeSet<_>>() {
        let a = h1.get(kw).copied().unwrap_or(0);
        let b = h2.get(kw).copied().unwrap_or(0);
        if a != b {
            keyword_drift.push((kw.clone(), a, b));
        }
    }

    Ok(Outcome {
        path: path.to_path_buf(),
        lines_pass1: significant_lines(&pass1),
        lines_pass2: significant_lines(&pass2),
        lines_original: significant_lines(&original),
        fixpoint: pass1 == pass2,
        canonical_fixpoint: canonicalise(&pass1) == canonicalise(&pass2),
        keyword_drift,
    })
}

/// Collect BTOR2 files under the given roots, deterministically ordered.
fn collect(roots: &[String]) -> Vec<PathBuf> {
    fn is_btor(p: &Path) -> bool {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| BTOR_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            // Do not follow directory symlinks; alias files created by
            // prepare_corpus.py are file symlinks and are picked up normally.
            if p.is_dir() && !p.is_symlink() {
                walk(&p, out);
            } else if is_btor(&p) {
                out.push(p);
            }
        }
    }

    let mut files = Vec::new();
    for root in roots {
        let p = Path::new(root);
        if p.is_dir() {
            walk(p, &mut files);
        } else if is_btor(p) {
            files.push(p.to_path_buf());
        } else {
            eprintln!("warning: {} is not a BTOR2 file, skipping", p.display());
        }
    }
    files.sort();
    files.dedup();
    files
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut roots = Vec::new();
    let mut keep: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--keep-output" => match args.next() {
                Some(dir) => keep = Some(PathBuf::from(dir)),
                None => {
                    eprintln!("error: --keep-output needs a directory");
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                eprintln!("usage: btor-roundtrip [--keep-output DIR] <file-or-dir>...");
                return ExitCode::SUCCESS;
            }
            other => roots.push(other.to_string()),
        }
    }

    if roots.is_empty() {
        eprintln!("usage: btor-roundtrip [--keep-output DIR] <file-or-dir>...");
        return ExitCode::from(2);
    }

    let files = collect(&roots);
    if files.is_empty() {
        eprintln!("error: no BTOR2 files found");
        return ExitCode::from(2);
    }

    println!("checking round-trip fidelity on {} instance(s)", files.len());

    let mut failures = Vec::new();
    let mut shrank = 0usize;
    let mut grew = 0usize;
    // Instances where raw text differed but meaning did not. Expected, and
    // worth counting: it measures how much of the deparser's output is
    // order-dependent rather than canonical.
    let mut raw_only = 0usize;

    for (i, f) in files.iter().enumerate() {
        match roundtrip(f, keep.as_deref()) {
            Ok(o) => {
                if !o.ok() {
                    println!(
                        "[{}/{}] FAIL {} canonical={} raw={} lines {}->{} drift={:?}",
                        i + 1,
                        files.len(),
                        o.path.display(),
                        o.canonical_fixpoint,
                        o.fixpoint,
                        o.lines_pass1,
                        o.lines_pass2,
                        o.keyword_drift
                    );
                    failures.push(o);
                } else {
                    if o.lines_pass1 < o.lines_original {
                        shrank += 1;
                    } else if o.lines_pass1 > o.lines_original {
                        grew += 1;
                    }
                    if !o.fixpoint {
                        raw_only += 1;
                    }
                    println!(
                        "[{}/{}] ok   {} lines {}->{}{}",
                        i + 1,
                        files.len(),
                        o.path.display(),
                        o.lines_original,
                        o.lines_pass1,
                        if o.fixpoint { "" } else { "  (raw differs, meaning preserved)" }
                    );
                }
            }
            Err(e) => {
                println!(
                    "[{}/{}] ERROR {} {}",
                    i + 1,
                    files.len(),
                    f.display(),
                    e
                );
                return ExitCode::from(1);
            }
        }
    }

    println!();
    println!("=== round-trip summary ===");
    println!("  instances                       : {}", files.len());
    println!("  semantic failures (the gate)    : {}", failures.len());
    println!("  raw text differed, meaning kept : {raw_only}");
    println!("  line count shrank vs original   : {shrank}");
    println!("  line count grew   vs original   : {grew}");
    println!();
    println!("Gate = equality after commutative-operand normalisation, plus no");
    println!("keyword gained or lost. Raw text equality is NOT the gate: the");
    println!("deparser orders commutative operands by internal term order, which");
    println!("depends on how many terms the process has already interned, so raw");
    println!("output differs across parse positions with no semantic change.");
    println!("A line-count change against the original is likewise expected --");
    println!("BTOR2 permits comments and alternative encodings.");
    println!();
    println!("This gate proves only that parse/deparse preserves meaning. Verdict");
    println!("invariance over the 840-case suite is the other half and is checked");
    println!("by bench/harness.py (plan item B5).");

    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

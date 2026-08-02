//! Scans this crate's own source for a defect class that has shipped three
//! times in one session: a string literal split across lines with `\` meant
//! to fold the newline away, where the continuation silently did not happen
//! (wrong indentation before the backslash, a missing backslash, or one
//! swallowed by a scripted edit). The literal then carries a raw newline plus
//! a run of leading-whitespace spaces in the middle of a sentence — invisible
//! in an editor that soft-wraps, glaring in the rendered UI.
//!
//! This is a source-text check, not a semantic one: it does not parse Rust,
//! it greps. That is deliberate — the bug is about what is literally between
//! the quotes, and a real parser would have to reconstruct exactly the naive
//! view this test takes.

use std::path::Path;

/// Finds every double-quoted string literal in `src` and returns the ones
/// containing a run of 4+ consecutive spaces — the signature of a failed
/// line-continuation, since no hand-written UI string legitimately needs
/// that much padding.
///
/// Has to walk `//` and `/* */` comments and `'x'` char literals as their own
/// states rather than ignore them, because a `"` inside a comment (routine —
/// this file's own doc comments quote example messages) would otherwise read
/// as the start of a string literal and swallow real code, indentation and
/// all, until some unrelated later `"` happened to re-sync it. The first
/// version of this scanner had exactly that bug, and its false positives were
/// the same shape as the defect it was built to catch — a lesson on trusting
/// a naive scanner's "found something" without reading what it found.
fn suspicious_literals(src: &str) -> Vec<(usize, String)> {
    #[derive(PartialEq)]
    enum St {
        Code,
        Line,
        Block(u32),
    }
    let mut out = Vec::new();
    let b = src.as_bytes();
    let (mut i, mut line, mut st) = (0usize, 1usize, St::Code);
    while i < b.len() {
        match st {
            St::Line => {
                if b[i] == b'\n' {
                    st = St::Code;
                }
                if b[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            St::Block(depth) => {
                if b[i..].starts_with(b"/*") {
                    st = St::Block(depth + 1);
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    st = if depth <= 1 {
                        St::Code
                    } else {
                        St::Block(depth - 1)
                    };
                    i += 2;
                } else {
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            St::Code => {
                if b[i..].starts_with(b"//") {
                    st = St::Line;
                    i += 2;
                } else if b[i..].starts_with(b"/*") {
                    st = St::Block(1);
                    i += 2;
                } else if b[i] == b'\'' {
                    // A char literal always has a closing `'` within a couple
                    // of bytes; a lifetime (`'a`) never does. That distinction
                    // is what stops `'"'` — an apostrophe, a double quote, an
                    // apostrophe — from being misread as a string starting.
                    let rest = &b[i + 1..];
                    let close = if rest.first() == Some(&b'\\') {
                        rest.iter().take(8).position(|&c| c == b'\'').map(|p| p + 1)
                    } else {
                        (rest.first() != Some(&b'\''))
                            .then(|| rest.first().map(|_| 1))
                            .flatten()
                    };
                    match close.filter(|&p| rest.get(p) == Some(&b'\'')) {
                        Some(p) => i += 2 + p,
                        None => i += 1,
                    }
                } else if b[i] == b'"' {
                    let start_line = line;
                    let mut j = i + 1;
                    let mut content = String::new();
                    while j < b.len() && b[j] != b'"' {
                        if b[j] == b'\\' && j + 1 < b.len() {
                            // The one case this test exists to catch: a
                            // continuation backslash must be immediately
                            // followed by a newline, which is folded away
                            // along with the next line's leading whitespace.
                            // CRLF: the file is Windows line-endings, so the
                            // continuation is `\` + `\r` + `\n`, not just
                            // `\n`. Missing the `\r` here was itself a copy of
                            // the bug this test exists to catch — checking
                            // only `\n` silently mis-scanned every real
                            // continuation in the file and produced false
                            // positives shaped exactly like the defect.
                            if b[j + 1] == b'\n' || b[j + 1..].starts_with(b"\r\n") {
                                j += if b[j + 1] == b'\r' { 3 } else { 2 };
                                line += 1;
                                while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                                    j += 1;
                                }
                                continue;
                            }
                            content.push(b[j] as char);
                            if j + 1 < b.len() {
                                content.push(b[j + 1] as char);
                            }
                            j += 2;
                            continue;
                        }
                        if b[j] == b'\n' {
                            line += 1;
                        }
                        content.push(b[j] as char);
                        j += 1;
                    }
                    // Not gated on the literal spanning multiple source
                    // lines — that was tried, and it hid the very bug this
                    // test was written for: the fillet/chamfer/TTR/TTT/
                    // explode/join/outline messages fixed alongside this test
                    // are each ONE physical source line carrying 40+ literal
                    // spaces, not a failed multi-line fold at all.
                    //
                    // The threshold is 10 spaces, not 4: a deliberate,
                    // hand-aligned label — "Chamfer      Fillet",
                    // `view.rs:1527` — runs to 6, which is the largest
                    // legitimate run found in this crate. A run in the tens
                    // is what a Rust indentation level leaking into a string
                    // looks like, never what a person aligning two words
                    // types.
                    if longest_run(&content, b' ') >= 10 {
                        out.push((start_line, content));
                    }
                    i = j + 1;
                } else {
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        }
    }
    out
}

#[test]
fn no_source_string_carries_an_unfolded_line_continuation() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut failures = Vec::new();
    for entry in walk(&src_dir) {
        let text = std::fs::read_to_string(&entry).unwrap_or_default();
        for (line, literal) in suspicious_literals(&text) {
            // Trim to a readable snippet centred on the run of spaces, so the
            // failure output points straight at the problem.
            let at = literal.find("    ").unwrap_or(0);
            let lo = at.saturating_sub(20);
            let hi = (at + 24).min(literal.len());
            failures.push(format!(
                "{}:{line}  …{}…",
                entry.display(),
                &literal[lo..hi]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "found {} string literal(s) with an unfolded line continuation \
         (4+ consecutive spaces mid-string — check the `\\` continuation \
         actually precedes a newline with nothing after it):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The length of the longest run of `target` bytes in `s`.
fn longest_run(s: &str, target: u8) -> usize {
    s.bytes()
        .fold((0usize, 0usize), |(best, cur), b| {
            let cur = if b == target { cur + 1 } else { 0 };
            (best.max(cur), cur)
        })
        .0
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

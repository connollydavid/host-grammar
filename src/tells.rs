//! Agentic-tell detection — a token-free English adaptation of the tropes
//! catalogued at tropes.fyi/tropes-md (Ossama, ossama.is). A *tell* is a
//! stylistic device that, in isolation is harmless but, piled up, reads as
//! machine-generated prose. We score density rather than ban devices: one
//! tricolon is rhetoric; five in a paragraph is a tell.
//!
//! Three layers (all token-free — no model, no word list licensing):
//!   1. Lexical: phrase/character rules (`CORPUS`).
//!   2. Structural: windowed heuristics with explicit equations (`scan_prose`).
//!   3. Composite: a per-document density `Score` (`tell_score`).
//!
//! References. Catalog: tropes.fyi/tropes-md (Ossama). Classical rhetoric terms
//! (anaphora, tricolon, anadiplosis) carry their standard meaning; tropes.fyi is
//! cited as the catalog, not as primary linguistics (it is itself AI-assisted).

use unicode_segmentation::UnicodeSegmentation;

/// Where a tell came from — a fixed phrase, or a structural equation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A phrase or character matched verbatim from `CORPUS`.
    Lexical,
    /// A windowed heuristic with an equation (negative parallelism, tricolon…).
    Structural,
}

/// One detected tell.
#[derive(Clone, Debug)]
pub struct Tell {
    /// Stable trope id, kebab-case (`negative-parallelism`).
    pub id: &'static str,
    pub kind: Kind,
    /// The matched text (a phrase, or the offending sentence/run).
    pub excerpt: String,
    /// Severity weight; larger contributes more to the document score.
    pub weight: f32,
    /// Citation: catalog name + rhetoric term where one exists.
    pub cite: &'static str,
}

/// Per-document aggregate — implements tropes.fyi's "the problem is many
/// together": individual tells are advisory, the density is what escalates.
#[derive(Clone, Copy, Debug)]
pub struct Score {
    pub tells: usize,
    pub weighted: f32,
    pub sentences: usize,
    /// `weighted / max(1, sentences)`.
    pub density: f32,
    /// True once density or absolute weight crosses the conservative gate.
    pub over_threshold: bool,
}

// A lexical corpus entry: one trope, the phrases that signal it, a weight, and a
// citation. Phrases are lowercase; matching is word-boundaried and case-folded.
struct Lexeme {
    id: &'static str,
    phrases: &'static [&'static str],
    weight: f32,
    cite: &'static str,
}

// The lexical layer. Token-free: these are public-domain English phrases, not a
// licensed lexicon. Weights are deliberately low — one word is never a verdict.
const CORPUS: &[Lexeme] = &[
    Lexeme {
        id: "ai-diction",
        phrases: &[
            "delve", "utilize", "leverage", "robust", "streamline", "harness",
            "tapestry", "landscape", "realm", "paradigm", "synergy", "ecosystem",
            "underscore", "showcase", "intricate", "nuanced", "multifaceted",
        ],
        weight: 0.5,
        cite: "tropes.fyi: AI vocabulary",
    },
    Lexeme {
        id: "magic-adverb",
        phrases: &["deeply", "fundamentally", "remarkably", "profoundly", "crucially"],
        weight: 0.4,
        cite: "tropes.fyi: intensifier inflation",
    },
    Lexeme {
        id: "serves-as",
        phrases: &["serves as", "stands as", "represents a", "acts as a"],
        weight: 0.6,
        cite: "tropes.fyi: copula dodge",
    },
    Lexeme {
        id: "filler-transition",
        phrases: &[
            "it's worth noting", "it is worth noting", "it bears mentioning",
            "importantly", "notably", "needless to say",
        ],
        weight: 0.6,
        cite: "tropes.fyi: empty signpost",
    },
    Lexeme {
        id: "signposted-conclusion",
        phrases: &["in conclusion", "to sum up", "in summary", "all in all"],
        weight: 0.7,
        cite: "tropes.fyi: signposted conclusion",
    },
    Lexeme {
        id: "pedagogical-hook",
        phrases: &[
            "let's unpack", "let's dive in", "let's dive into", "let's break this down",
            "here's the kicker", "here's the thing", "buckle up",
        ],
        weight: 0.8,
        cite: "tropes.fyi: false suspense / pedagogical hook",
    },
    Lexeme {
        id: "decoration",
        // ASCII-decoration tells: em/en-dash, smart quotes, arrows. Matched as
        // characters, so the boundary rule is relaxed for these (see `matches`).
        phrases: &["—", "–", "“", "”", "‘", "’", "→"],
        weight: 0.3,
        cite: "tropes.fyi: typographic polish",
    },
];

// Negation openers and contrastive pivots for the negative-parallelism window.
const NEGATIONS: &[&str] = &["not", "isn't", "isnt", "doesn't", "doesnt", "won't", "wont", "never", "no"];
const PIVOTS: &[&str] = &["it's", "its", "but", "rather", "instead", "—", "than"];
const ORDINALS: &[&str] = &["first", "second", "third", "fourth", "fifth", "next", "finally", "lastly"];
// Leading words skipped when computing a sentence's anaphora "opener".
const STOPWORDS: &[&str] = &["the", "a", "an", "and", "but", "so", "yet", "for", "this", "that", "it"];

fn lc_words(s: &str) -> Vec<String> {
    s.unicode_words().map(|w| w.to_lowercase()).collect()
}

// Lexical scan: every word-boundaried, case-folded corpus phrase. Decoration
// chars match anywhere (they have no word boundary).
fn lexical(text: &str, out: &mut Vec<Tell>) {
    let lower = text.to_lowercase();
    for lex in CORPUS {
        for phrase in lex.phrases {
            let decoration = lex.id == "decoration";
            let mut from = 0;
            while let Some(rel) = lower[from..].find(phrase) {
                let at = from + rel;
                let end = at + phrase.len();
                let lb = lower.as_bytes();
                let left = at == 0 || !lb[at - 1].is_ascii_alphanumeric();
                let right = end == lb.len() || !lb[end].is_ascii_alphanumeric();
                if decoration || (left && right) {
                    out.push(Tell {
                        id: lex.id,
                        kind: Kind::Lexical,
                        excerpt: text[at..end].to_string(),
                        weight: lex.weight,
                        cite: lex.cite,
                    });
                    if decoration {
                        // one decoration tell per phrase is enough signal
                        break;
                    }
                }
                from = end;
            }
        }
    }
}

fn sentences(text: &str) -> Vec<&str> {
    text.unicode_sentences().map(str::trim).filter(|s| !s.is_empty()).collect()
}

// Negative parallelism: a negation followed within a short window by a
// contrastive pivot — "it's not X, it's Y", "not slower, but faster".
//   neg_par(s) = | { (i,j) : word_i ∈ NEG, word_j ∈ PIVOT, 0 < j-i ≤ W } |
fn negative_parallelism(sentence: &str) -> usize {
    const W: usize = 6;
    let words = lc_words(sentence);
    let mut hits = 0;
    for (i, w) in words.iter().enumerate() {
        if NEGATIONS.contains(&w.as_str()) {
            let upper = (i + W).min(words.len().saturating_sub(1));
            if words[i + 1..=upper].iter().any(|p| PIVOTS.contains(&p.as_str())) {
                hits += 1;
            }
        }
    }
    hits
}

// Tricolon: a parallel triad "A, B, and C" whose three spans are short and
// comparable. is_triad(s) = 1 when the sentence has ≥2 commas, an "and"/"or"
// before the final item, and each of the three comma-spans is ≤ 4 words with no
// internal terminal punctuation.
fn is_triad(sentence: &str) -> bool {
    let trimmed = sentence.trim_end_matches(['.', '!', '?', ' ']);
    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return false;
    }
    let last = parts.last().unwrap();
    let lw = lc_words(last);
    if lw.first().map(|w| w != "and" && w != "or").unwrap_or(true) {
        return false;
    }
    parts.iter().all(|p| {
        let n = lc_words(p).len();
        n >= 1 && n <= 5 && !p.contains(['.', '!', '?', ';'])
    })
}

// Anaphora: consecutive sentences sharing an opener (first content word after a
// leading stopword). A run of length L scores max(0, L-2)² — superlinear so a
// pair is free and a marching list is heavy.
//   anaphora = Σ_runs max(0, L_r − 2)²
fn opener(sentence: &str) -> Option<String> {
    let words = lc_words(sentence);
    let mut it = words.into_iter();
    let mut first = it.next()?;
    if STOPWORDS.contains(&first.as_str()) {
        first = it.next()?;
    }
    Some(first)
}

fn anaphora_runs(sents: &[&str]) -> Vec<(String, usize)> {
    let mut runs: Vec<(String, usize)> = Vec::new();
    let mut cur: Option<String> = None;
    let mut len = 0usize;
    for s in sents {
        match opener(s) {
            Some(o) if Some(&o) == cur.as_ref() => len += 1,
            other => {
                if let (Some(o), true) = (cur.take(), len >= 3) {
                    runs.push((o, len));
                }
                cur = other;
                len = 1;
            }
        }
    }
    if let (Some(o), true) = (cur, len >= 3) {
        runs.push((o, len));
    }
    runs
}

// "Not X. Not Y. Just Z." — a run of ≥2 sentence-initial "Not …" fragments
// closed by a "Just/Only …". countdown = max(0, run_len) on close.
fn countdown(sents: &[&str]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut run = 0usize;
    for s in sents {
        let first = lc_words(s).into_iter().next().unwrap_or_default();
        if first == "not" || first == "no" {
            run += 1;
        } else if (first == "just" || first == "only") && run >= 2 {
            out.push(run + 1);
            run = 0;
        } else {
            run = 0;
        }
    }
    out
}

// Self-answered question: a "?"-terminated sentence immediately followed by a
// short fragment (≤ 5 words). Counts adjacencies.
fn self_answered(sents: &[&str]) -> usize {
    let mut hits = 0;
    for pair in sents.windows(2) {
        if pair[0].ends_with('?') {
            let n = lc_words(pair[1]).len();
            if n >= 1 && n <= 5 {
                hits += 1;
            }
        }
    }
    hits
}

// Listicle-in-prose: consecutive sentences led by ordinals. An anaphora variant
// over ORDINALS; a run ≥3 scores like anaphora.
fn listicle(sents: &[&str]) -> usize {
    let mut run = 0usize;
    let mut score = 0usize;
    let mut flush = |run: usize| if run >= 3 { (run - 2) * (run - 2) } else { 0 };
    for s in sents {
        let first = lc_words(s).into_iter().next().unwrap_or_default();
        if ORDINALS.contains(&first.as_str()) {
            run += 1;
        } else {
            score += flush(run);
            run = 0;
        }
    }
    score + flush(run)
}

// Superficial "-ing" tail: a trailing participial clause ", verbing …" at a
// sentence end (… , highlighting the importance).
fn ing_tail(sentence: &str) -> bool {
    let Some((_, tail)) = sentence.rsplit_once(',') else { return false };
    let words = lc_words(tail);
    words.first().map(|w| w.ends_with("ing") && w.len() > 4).unwrap_or(false)
}

// False range: "from X to Y" density (count only; whether the spectrum is valid
// needs semantics, so we count rather than judge).
fn false_ranges(sentence: &str) -> usize {
    let words = lc_words(sentence);
    let mut hits = 0;
    for (i, w) in words.iter().enumerate() {
        if w == "from" && words[i + 1..].iter().take(5).any(|x| x == "to") {
            hits += 1;
        }
    }
    hits
}

// Paragraph-level shape tells: punchy single-sentence paragraphs and bold-first
// bullets. Returned as weighted Tells.
fn shape(text: &str, out: &mut Vec<Tell>) {
    let paras: Vec<&str> = text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).collect();
    if paras.len() >= 4 {
        let single = paras.iter().filter(|p| !p.contains('\n') && sentences(p).len() <= 1).count();
        let ratio = single as f32 / paras.len() as f32;
        if ratio >= 0.5 {
            out.push(Tell {
                id: "punchy-fragments",
                kind: Kind::Structural,
                excerpt: format!("{single}/{} paragraphs are single-sentence", paras.len()),
                weight: ratio * 2.0,
                cite: "tropes.fyi: staccato paragraphs",
            });
        }
    }
    let bullets: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("- ") || l.starts_with("* ") || l.starts_with("+ "))
        .collect();
    if bullets.len() >= 3 {
        let bold = bullets.iter().filter(|l| l[2..].trim_start().starts_with("**")).count();
        let ratio = bold as f32 / bullets.len() as f32;
        if ratio >= 0.6 {
            out.push(Tell {
                id: "bold-first-bullets",
                kind: Kind::Structural,
                excerpt: format!("{bold}/{} bullets open with bold lead-ins", bullets.len()),
                weight: ratio * 1.5,
                cite: "tropes.fyi: bold-lead bullets",
            });
        }
    }
}

/// Scan prose for every detectable agentic tell (lexical + structural).
pub fn scan_prose(text: &str) -> Vec<Tell> {
    let mut out = Vec::new();
    lexical(text, &mut out);
    let sents = sentences(text);

    for s in &sents {
        for _ in 0..negative_parallelism(s) {
            out.push(Tell {
                id: "negative-parallelism",
                kind: Kind::Structural,
                excerpt: (*s).to_string(),
                weight: 1.2,
                cite: "tropes.fyi: negative parallelism (antithesis)",
            });
        }
        if is_triad(s) {
            out.push(Tell {
                id: "tricolon",
                kind: Kind::Structural,
                excerpt: (*s).to_string(),
                weight: 0.9,
                cite: "tropes.fyi: tricolon (classical rhetoric)",
            });
        }
        if ing_tail(s) {
            out.push(Tell {
                id: "ing-tail",
                kind: Kind::Structural,
                excerpt: (*s).to_string(),
                weight: 0.7,
                cite: "tropes.fyi: participial tail",
            });
        }
        for _ in 0..false_ranges(s) {
            out.push(Tell {
                id: "false-range",
                kind: Kind::Structural,
                excerpt: (*s).to_string(),
                weight: 0.5,
                cite: "tropes.fyi: false range / spectrum",
            });
        }
    }

    for (op, len) in anaphora_runs(&sents) {
        let w = ((len - 2) * (len - 2)) as f32;
        out.push(Tell {
            id: "anaphora",
            kind: Kind::Structural,
            excerpt: format!("{len} sentences open with \"{op}\""),
            weight: w,
            cite: "tropes.fyi: anaphora (classical rhetoric)",
        });
    }
    for len in countdown(&sents) {
        out.push(Tell {
            id: "countdown",
            kind: Kind::Structural,
            excerpt: format!("{len}-part \"Not…/Just…\" countdown"),
            weight: 1.3,
            cite: "tropes.fyi: countdown / triadic close",
        });
    }
    let sa = self_answered(&sents);
    for _ in 0..sa {
        out.push(Tell {
            id: "self-answered-question",
            kind: Kind::Structural,
            excerpt: "rhetorical question answered by a fragment".to_string(),
            weight: 1.0,
            cite: "tropes.fyi: self-answered question (hypophora)",
        });
    }
    let lst = listicle(&sents);
    if lst > 0 {
        out.push(Tell {
            id: "listicle",
            kind: Kind::Structural,
            excerpt: "ordinal-led sentence run".to_string(),
            weight: lst as f32,
            cite: "tropes.fyi: listicle-in-prose",
        });
    }

    shape(text, &mut out);
    out
}

/// Conservative gate: a document is over-threshold when its weighted tell mass
/// is high in absolute terms *and* dense relative to its length. Starting
/// values are intentionally cautious; tune with fixtures.
const ABS_GATE: f32 = 4.0;
const DENSITY_GATE: f32 = 0.6;

/// Aggregate the tells of `text` into a document `Score`.
pub fn tell_score(text: &str) -> Score {
    let tells = scan_prose(text);
    let weighted: f32 = tells.iter().map(|t| t.weight).sum();
    let sentences = sentences(text).len().max(1);
    let density = weighted / sentences as f32;
    Score {
        tells: tells.len(),
        weighted,
        sentences,
        density,
        over_threshold: weighted >= ABS_GATE && density >= DENSITY_GATE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(text: &str) -> Vec<&'static str> {
        scan_prose(text).into_iter().map(|t| t.id).collect()
    }

    #[test]
    fn lexical_fires_on_diction_not_on_clean() {
        assert!(ids("We delve into the rich tapestry.").contains(&"ai-diction"));
        assert!(!ids("We read the source file carefully.").contains(&"ai-diction"));
    }

    #[test]
    fn lexical_is_word_boundaried() {
        // "delved" / "delusion" must not match "delve".
        assert!(!ids("The delusion deepened.").contains(&"ai-diction"));
    }

    #[test]
    fn negative_parallelism_fires_on_pivot_not_plain_negation() {
        assert!(ids("It's not a bug, it's a feature.").contains(&"negative-parallelism"));
        assert!(!ids("This does not compile.").contains(&"negative-parallelism"));
    }

    #[test]
    fn tricolon_detects_short_triad() {
        assert!(ids("It is fast, clean, and simple.").contains(&"tricolon"));
        // a long, clause-y list is not a tight tricolon
        assert!(!ids("We refactored the parser, then we rewrote the entire scanner from scratch, and finally shipped.").contains(&"tricolon"));
    }

    #[test]
    fn anaphora_penalizes_runs_of_three() {
        let t = "We build the thing. We test the thing. We ship the thing.";
        assert!(ids(t).contains(&"anaphora"));
        let two = "We build it. We test it. Then everyone goes home.";
        assert!(!ids(two).contains(&"anaphora"));
    }

    #[test]
    fn countdown_and_self_answered() {
        assert!(ids("Not magic. Not luck. Just engineering.").contains(&"countdown"));
        assert!(ids("The result? A clean build.").contains(&"self-answered-question"));
    }

    #[test]
    fn clean_prose_stays_under_threshold() {
        let clean = "The parser reads each line and reports the first tell it finds. \
                     Lines in code files are checked the same way as markdown. \
                     A missing allow-list file simply means no phrases are masked.";
        assert!(!tell_score(clean).over_threshold);
    }

    #[test]
    fn trope_dense_paragraph_crosses_threshold() {
        let dense = "Let's unpack this. It's not a tweak, it's a revolution. \
                     We delve. We leverage. We harness. The result? Pure synergy. \
                     Fast, clean, and robust.";
        assert!(tell_score(dense).over_threshold);
    }
}

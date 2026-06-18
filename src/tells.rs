//! Agentic-tell detection: a token-free adaptation of the tropes.fyi catalog.
//!
//! Three layers, all token-free (no model, no licensed lexicon):
//!   1. Lexical: phrase/character rules (`CORPUS`).
//!   2. Structural: windowed equations (`scan_prose`).
//!   3. Composite: a per-document density `Score` (`tell_score`).
//!
//! Catalog cite: tropes.fyi/tropes-md (Ossama, ossama.is).

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

/// Per-document aggregate: tell counts and the density gate.
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

// One trope: signal phrases (lowercase), a weight, a citation.
struct Lexeme {
    id: &'static str,
    phrases: &'static [&'static str],
    weight: f32,
    cite: &'static str,
}

// Lexical layer: public-domain phrases, low weights (one word is never a verdict).
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
        // Typographic tells; matched as characters (no word boundary, see `lexical`).
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
                        // Index into `lower`, not `text`: case-folding can change
                        // byte lengths, so `at`/`end` are only valid in `lower`.
                        excerpt: lower[at..end].to_string(),
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

// One sentence's precomputed cross-sentence signals — the unit a parallel worker
// produces for its chunk. Concatenating chunk Metas in chunk order reproduces the
// whole-document sequence, so a run straddling a chunk boundary rejoins under
// concatenation (the associative merge of `call/0015`).
struct Meta {
    opener: Option<String>, // first content word past a leading stopword (anaphora)
    first: String,          // first lowercased word (countdown, listicle); "" if none
    is_question: bool,      // ends with '?'
    words: usize,           // word count (self-answered fragment bound)
}

fn sentence_meta(s: &str) -> Meta {
    let words = lc_words(s);
    let first = words.first().cloned().unwrap_or_default();
    let opener = match words.first() {
        Some(w) if STOPWORDS.contains(&w.as_str()) => words.get(1).cloned(),
        other => other.cloned(),
    };
    Meta { opener, first, is_question: s.ends_with('?'), words: words.len() }
}

// Anaphora: consecutive sentences sharing an opener (first content word after a
// leading stopword). A run of length L scores max(0, L-2)² — superlinear so a
// pair is free and a marching list is heavy.
//   anaphora = Σ_runs max(0, L_r − 2)²
fn anaphora_runs(metas: &[Meta]) -> Vec<(String, usize)> {
    let mut runs: Vec<(String, usize)> = Vec::new();
    let mut cur: Option<String> = None;
    let mut len = 0usize;
    for m in metas {
        match &m.opener {
            Some(o) if Some(o) == cur.as_ref() => len += 1,
            other => {
                if let (Some(o), true) = (cur.take(), len >= 3) {
                    runs.push((o, len));
                }
                cur = other.clone();
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
fn countdown(metas: &[Meta]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut run = 0usize;
    for m in metas {
        match m.first.as_str() {
            "not" | "no" => run += 1,
            "just" | "only" if run >= 2 => {
                out.push(run + 1);
                run = 0;
            }
            _ => run = 0,
        }
    }
    out
}

// Self-answered question: a "?"-terminated sentence immediately followed by a
// short fragment (≤ 5 words). Counts adjacencies.
fn self_answered(metas: &[Meta]) -> usize {
    metas
        .windows(2)
        .filter(|p| p[0].is_question && p[1].words >= 1 && p[1].words <= 5)
        .count()
}

// Listicle-in-prose: consecutive sentences led by ordinals. An anaphora variant
// over ORDINALS; a run ≥3 scores like anaphora.
fn listicle(metas: &[Meta]) -> usize {
    let mut run = 0usize;
    let mut score = 0usize;
    let flush = |run: usize| if run >= 3 { (run - 2) * (run - 2) } else { 0 };
    for m in metas {
        if ORDINALS.contains(&m.first.as_str()) {
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
        if ratio > 0.5 {
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

// Per-sentence structural tells (independent of neighbours) — the embarrassingly
// parallel unit, appended in sentence order.
fn sentence_tells(s: &str, out: &mut Vec<Tell>) {
    for _ in 0..negative_parallelism(s) {
        out.push(Tell {
            id: "negative-parallelism",
            kind: Kind::Structural,
            excerpt: s.to_string(),
            weight: 1.2,
            cite: "tropes.fyi: negative parallelism (antithesis)",
        });
    }
    if is_triad(s) {
        out.push(Tell {
            id: "tricolon",
            kind: Kind::Structural,
            excerpt: s.to_string(),
            weight: 0.9,
            cite: "tropes.fyi: tricolon (classical rhetoric)",
        });
    }
    if ing_tail(s) {
        out.push(Tell {
            id: "ing-tail",
            kind: Kind::Structural,
            excerpt: s.to_string(),
            weight: 0.7,
            cite: "tropes.fyi: participial tail",
        });
    }
    for _ in 0..false_ranges(s) {
        out.push(Tell {
            id: "false-range",
            kind: Kind::Structural,
            excerpt: s.to_string(),
            weight: 0.5,
            cite: "tropes.fyi: false range / spectrum",
        });
    }
}

// Cross-sentence run equations over the merged per-sentence metadata.
fn run_tells(metas: &[Meta], out: &mut Vec<Tell>) {
    for (op, len) in anaphora_runs(metas) {
        out.push(Tell {
            id: "anaphora",
            kind: Kind::Structural,
            excerpt: format!("{len} sentences open with \"{op}\""),
            weight: ((len - 2) * (len - 2)) as f32,
            cite: "tropes.fyi: anaphora (classical rhetoric)",
        });
    }
    for len in countdown(metas) {
        out.push(Tell {
            id: "countdown",
            kind: Kind::Structural,
            excerpt: format!("{len}-part \"Not…/Just…\" countdown"),
            weight: 1.3,
            cite: "tropes.fyi: countdown / triadic close",
        });
    }
    for _ in 0..self_answered(metas) {
        out.push(Tell {
            id: "self-answered-question",
            kind: Kind::Structural,
            excerpt: "rhetorical question answered by a fragment".to_string(),
            weight: 1.0,
            cite: "tropes.fyi: self-answered question (hypophora)",
        });
    }
    let lst = listicle(metas);
    if lst > 0 {
        out.push(Tell {
            id: "listicle",
            kind: Kind::Structural,
            excerpt: "ordinal-led sentence run".to_string(),
            weight: lst as f32,
            cite: "tropes.fyi: listicle-in-prose",
        });
    }
}

// Assemble the document result from per-sentence tells (in sentence order) and
// the merged metadata. Output order: lexical, per-sentence, run, shape — the one
// order both the sequential and parallel paths produce.
fn assemble(text: &str, per_sentence: Vec<Tell>, metas: &[Meta]) -> Vec<Tell> {
    let mut out = Vec::new();
    lexical(text, &mut out);
    out.extend(per_sentence);
    run_tells(metas, &mut out);
    shape(text, &mut out);
    out
}

/// Scan prose for every detectable agentic tell (lexical + structural).
pub fn scan_prose(text: &str) -> Vec<Tell> {
    let sents = sentences(text);
    let mut per_sentence = Vec::new();
    let mut metas = Vec::with_capacity(sents.len());
    for s in &sents {
        sentence_tells(s, &mut per_sentence);
        metas.push(sentence_meta(s));
    }
    assemble(text, per_sentence, &metas)
}

/// Below this sentence count, `scan_prose_parallel` stays sequential — thread
/// setup is not worth it for short documents and titles.
const PARALLEL_THRESHOLD: usize = 64;

/// Parallel scan for large documents. Result is identical to `scan_prose`; the
/// per-sentence tokenization is split across available cores above the threshold.
pub fn scan_prose_parallel(text: &str) -> Vec<Tell> {
    if sentences(text).len() < PARALLEL_THRESHOLD {
        return scan_prose(text);
    }
    let k = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    scan_chunked(text, k)
}

/// `scan_prose` computed in up to `k` contiguous sentence chunks, each scanned on
/// its own thread. For every `k >= 1` the result equals `scan_prose(text)` — the
/// invariant the allium/PBT lane checks and the TLA+ spec models across all
/// worker-completion interleavings. Public so the property lane can force `k`.
pub fn scan_chunked(text: &str, k: usize) -> Vec<Tell> {
    let sents = sentences(text);
    let k = k.min(sents.len()).max(1);
    if k <= 1 {
        return scan_prose(text);
    }
    let chunk = (sents.len() + k - 1) / k;
    // Each worker owns its chunk and shares no mutable state; it returns its
    // per-sentence tells and its metadata (the chunk's run summary).
    let partials: Vec<(Vec<Tell>, Vec<Meta>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = sents
            .chunks(chunk)
            .map(|slice| {
                scope.spawn(move || {
                    let mut ts = Vec::new();
                    let mut ms = Vec::with_capacity(slice.len());
                    for &s in slice {
                        sentence_tells(s, &mut ts);
                        ms.push(sentence_meta(s));
                    }
                    (ts, ms)
                })
            })
            .collect();
        // Join in chunk order (not completion order) so the merge is deterministic.
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut per_sentence = Vec::new();
    let mut metas = Vec::with_capacity(sents.len());
    for (ts, ms) in partials {
        per_sentence.extend(ts);
        metas.extend(ms);
    }
    assemble(text, per_sentence, &metas)
}

// === Markdown-aware scanning (plan/0010) ===

#[derive(PartialEq, Clone, Copy)]
enum BlockKind {
    Para,
    Heading,
    ListItem,
}

// A prose block extracted from markdown: its text with code, link targets, and
// markup removed. `bold_first` marks a list item whose first inline is bold.
struct Block {
    kind: BlockKind,
    text: String,
    bold_first: bool,
}

// Parse markdown into prose blocks, dropping code blocks and inline code and
// keeping only link/image visible text (not URLs).
fn parse_markdown(md: &str) -> Vec<Block> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let mut blocks: Vec<Block> = Vec::new();
    let mut cur: Option<Block> = None;
    let mut in_code = false;

    let flush = |cur: &mut Option<Block>, blocks: &mut Vec<Block>| {
        if let Some(b) = cur.take() {
            if !b.text.trim().is_empty() {
                blocks.push(b);
            }
        }
    };
    let start = |cur: &mut Option<Block>, blocks: &mut Vec<Block>, kind: BlockKind| {
        flush(cur, blocks);
        *cur = Some(Block { kind, text: String::new(), bold_first: false });
    };

    for ev in Parser::new(md) {
        match ev {
            Event::Start(Tag::CodeBlock(_)) => in_code = true,
            Event::End(TagEnd::CodeBlock) => in_code = false,
            Event::Start(Tag::Heading { .. }) => start(&mut cur, &mut blocks, BlockKind::Heading),
            Event::End(TagEnd::Heading(_)) => flush(&mut cur, &mut blocks),
            Event::Start(Tag::Item) => start(&mut cur, &mut blocks, BlockKind::ListItem),
            Event::End(TagEnd::Item) => flush(&mut cur, &mut blocks),
            Event::Start(Tag::Paragraph) => {
                // A paragraph inside a list item continues the item; otherwise it
                // opens a new prose paragraph.
                if cur.as_ref().map(|b| b.kind != BlockKind::ListItem).unwrap_or(true) {
                    start(&mut cur, &mut blocks, BlockKind::Para);
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if cur.as_ref().map(|b| b.kind == BlockKind::Para).unwrap_or(false) {
                    flush(&mut cur, &mut blocks);
                }
            }
            Event::Start(Tag::Strong) => {
                if let Some(b) = &mut cur {
                    if b.kind == BlockKind::ListItem && b.text.trim().is_empty() {
                        b.bold_first = true;
                    }
                }
            }
            Event::Text(t) => {
                if !in_code {
                    if let Some(b) = &mut cur {
                        b.text.push_str(&t);
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(b) = &mut cur {
                    b.text.push(' ');
                }
            }
            // Inline `code`, link/image URLs, and html are not prose: skipped.
            _ => {}
        }
    }
    flush(&mut cur, &mut blocks);
    blocks
}

// Shape tells computed from markdown block structure: headings are excluded from
// the paragraph count, and bold-first is read from the parse, not from `**`.
fn markdown_shape(blocks: &[Block], out: &mut Vec<Tell>) {
    let paras: Vec<&Block> = blocks.iter().filter(|b| b.kind == BlockKind::Para).collect();
    if paras.len() >= 4 {
        let single = paras.iter().filter(|b| sentences(&b.text).len() <= 1).count();
        let ratio = single as f32 / paras.len() as f32;
        if ratio > 0.5 {
            out.push(Tell {
                id: "punchy-fragments",
                kind: Kind::Structural,
                excerpt: format!("{single}/{} paragraphs are single-sentence", paras.len()),
                weight: ratio * 2.0,
                cite: "tropes.fyi: staccato paragraphs",
            });
        }
    }
    let bullets: Vec<&Block> = blocks.iter().filter(|b| b.kind == BlockKind::ListItem).collect();
    if bullets.len() >= 3 {
        let bold = bullets.iter().filter(|b| b.bold_first).count();
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

/// Scan a markdown document for prose tells. Unlike `scan_prose`, code blocks and
/// inline code are excluded, link URLs are dropped (visible text kept), and
/// headings are scanned for tells but not counted as prose paragraphs.
pub fn scan_prose_markdown(md: &str) -> Vec<Tell> {
    let blocks = parse_markdown(md);
    // Lexical (diction) is scanned over all block text, headings included — a
    // heading can read "Let's unpack…". The per-sentence and cross-sentence
    // equations run only over body blocks, so parallel headings ("Section one /
    // two / three") are not read as an anaphora run.
    let all_text = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n\n");
    let body = blocks
        .iter()
        .filter(|b| b.kind != BlockKind::Heading)
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let sents = sentences(&body);
    let mut per_sentence = Vec::new();
    let mut metas = Vec::with_capacity(sents.len());
    for s in &sents {
        sentence_tells(s, &mut per_sentence);
        metas.push(sentence_meta(s));
    }
    let mut out = Vec::new();
    lexical(&all_text, &mut out);
    out.extend(per_sentence);
    run_tells(&metas, &mut out);
    markdown_shape(&blocks, &mut out);
    out
}

// Over-threshold needs high absolute weight AND high density (conservative; tune with fixtures).
const ABS_GATE: f32 = 4.0;
const DENSITY_GATE: f32 = 0.6;

/// Aggregate the tells of `text` into a document `Score`.
pub fn tell_score(text: &str) -> Score {
    let tells = scan_prose_parallel(text);
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

    fn md_ids(md: &str) -> Vec<&'static str> {
        scan_prose_markdown(md).into_iter().map(|t| t.id).collect()
    }

    #[test]
    fn markdown_excludes_code_blocks_and_inline_code() {
        let fenced = "Intro paragraph.\n\n```\nIt's not a tweak, it's a revolution — we delve.\n```\n";
        assert!(md_ids(fenced).is_empty(), "{:?}", md_ids(fenced));
        assert!(md_ids("Use the `delve --tapestry` flag.").is_empty());
    }

    #[test]
    fn markdown_drops_link_urls_keeps_text() {
        // "delve" only in the URL path → clean; the visible text is ordinary.
        assert!(md_ids("See [the guide](https://x.test/delve/tapestry).").is_empty());
        // but diction in the visible text is still caught
        assert!(md_ids("See [delve into it](https://x.test/ok).").contains(&"ai-diction"));
    }

    #[test]
    fn markdown_headings_are_not_runs_or_paragraphs() {
        // parallel section headings must not read as an anaphora run
        let doc = "# Title\n\n## Section one\n\n## Section two\n\n## Section three\n";
        assert!(md_ids(doc).is_empty(), "{:?}", md_ids(doc));
        // a single short doc is not staccato (more-than-half rule)
        let short = "# T\n\nOne sentence intro.\n\nA body paragraph with two sentences. It keeps going.\n\nClosing line.\n";
        assert!(!md_ids(short).contains(&"punchy-fragments"));
    }

    #[test]
    fn markdown_still_scans_heading_diction() {
        assert!(md_ids("## Let's unpack the design\n\nOrdinary text.").contains(&"pedagogical-hook"));
    }
}

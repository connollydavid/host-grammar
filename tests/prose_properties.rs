// Property-based tests for the agentic-tell equations, one property per
// invariant in host-grammar.allium. Black-box over the public API (scan_prose /
// tell_score) so the properties verify the contract, not the implementation.

use host_grammar::{scan_chunked, scan_prose, tell_score, Tell};
use proptest::prelude::*;

// A Tell's identity for equality: id, exact weight bits, and excerpt.
fn key(t: &Tell) -> (&'static str, u32, String) {
    (t.id, t.weight.to_bits(), t.excerpt.clone())
}
fn keys(ts: &[Tell]) -> Vec<(&'static str, u32, String)> {
    ts.iter().map(key).collect()
}

fn tells_with(text: &str, id: &str) -> Vec<host_grammar::Tell> {
    scan_prose(text).into_iter().filter(|t| t.id == id).collect()
}

// Declarative reference for the anaphora equation: group an opener sequence into
// maximal runs, then Σ max(0, L-2)^2 over run lengths. Computed independently of
// the engine's streaming accumulator, so equality is a refinement check.
fn ref_anaphora_weight(openers: &[usize]) -> f32 {
    let mut lens: Vec<usize> = Vec::new();
    for (i, &o) in openers.iter().enumerate() {
        if i > 0 && openers[i - 1] == o {
            *lens.last_mut().unwrap() += 1;
        } else {
            lens.push(1);
        }
    }
    lens.iter().map(|&l| if l >= 3 { ((l - 2) * (l - 2)) as f32 } else { 0.0 }).sum()
}

// Build a document whose sentence openers are exactly `openers` (a small alphabet
// of non-stopwords), each sentence otherwise unique so only anaphora can fire.
// Sentences are capitalized: Unicode sentence segmentation (UAX#29) only treats
// ". " as a boundary before a capital, which is how real prose anaphora reads.
fn doc_from_openers(openers: &[usize]) -> String {
    const WORDS: [&str; 4] = ["We", "They", "Cats", "Dogs"];
    openers
        .iter()
        .enumerate()
        .map(|(i, &o)| format!("{} run{i}. ", WORDS[o % WORDS.len()]))
        .collect()
}

proptest! {
    // invariant AnaphoraSuperlinear: a run of L same-opener sentences (L >= 3)
    // yields exactly one anaphora tell of weight (L-2)^2.
    #[test]
    fn anaphora_weight_is_excess_squared(l in 3usize..=8) {
        let text: String = (0..l).map(|i| format!("We do thing{i}. ")).collect();
        let hits = tells_with(&text, "anaphora");
        prop_assert_eq!(hits.len(), 1, "text: {}", text);
        prop_assert_eq!(hits[0].weight, ((l - 2) * (l - 2)) as f32);
    }

    // invariant AnaphoraPairIsFree: L <= 2 never trips anaphora.
    #[test]
    fn anaphora_pair_is_free(l in 0usize..=2) {
        let text: String = (0..l).map(|i| format!("We do thing{i}. ")).collect();
        prop_assert!(tells_with(&text, "anaphora").is_empty(), "text: {}", text);
    }

    // rule Tricolon: a short comma triad is detected.
    #[test]
    fn short_triad_is_a_tricolon(a in "[a-z]{2,5}", b in "[a-z]{2,5}", c in "[a-z]{2,5}") {
        let text = format!("It is {a}, {b}, and {c}.");
        prop_assert!(!tells_with(&text, "tricolon").is_empty(), "text: {}", text);
    }

    // invariant TricolonSpanBound: a span over triad_max_span words is not a triad.
    #[test]
    fn long_span_is_not_a_tricolon(b in "[a-z]{2,5}", c in "[a-z]{2,5}") {
        // first span is nine words, well over the bound
        let text = format!("one two three four five six seven eight nine, {b}, and {c}.");
        prop_assert!(tells_with(&text, "tricolon").is_empty(), "text: {}", text);
    }

    // rule NegativeParallelism: a negation with a pivot in window fires.
    #[test]
    fn negation_with_pivot_fires(x in "[a-z]{3,8}", y in "[a-z]{3,8}") {
        let text = format!("It's not a {x}, it's a {y}.");
        prop_assert!(!tells_with(&text, "negative-parallelism").is_empty(), "text: {}", text);
    }

    // invariant NegationWithoutPivotIsClean: plain negation does not fire.
    #[test]
    fn negation_without_pivot_is_clean(v in "[a-z]{3,8}") {
        let text = format!("This does not {v} today.");
        prop_assert!(tells_with(&text, "negative-parallelism").is_empty(), "text: {}", text);
    }

    // rule Countdown: k Not-sentences (k >= 2) closed by Just fire once.
    #[test]
    fn countdown_fires_on_closed_run(k in 2usize..=5) {
        let mut text: String = (0..k).map(|i| format!("Not thing{i}. ")).collect();
        text.push_str("Just engineering.");
        prop_assert_eq!(tells_with(&text, "countdown").len(), 1, "text: {}", text);
    }

    // rule SelfAnsweredQuestion: a question + short fragment fires.
    #[test]
    fn question_plus_short_fragment_fires(x in "[a-z]{3,8}", y in "[a-z]{3,8}") {
        let text = format!("Is it a {x}? A {y}.");
        prop_assert!(!tells_with(&text, "self-answered-question").is_empty(), "text: {}", text);
    }

    // invariant SelfAnsweredFragmentBound: a long answer is not a fragment.
    #[test]
    fn question_plus_long_answer_is_clean(x in "[a-z]{3,8}") {
        let text = format!("Is it a {x}? It is a long and carefully detailed answer here.");
        prop_assert!(tells_with(&text, "self-answered-question").is_empty(), "text: {}", text);
    }

    // rule Listicle: a run of >= 3 ordinal-led sentences fires.
    #[test]
    fn ordinal_run_is_a_listicle(n in 3usize..=5) {
        let ords = ["First", "Second", "Third", "Fourth", "Fifth"];
        let text: String = (0..n).map(|i| format!("{} we move. ", ords[i])).collect();
        prop_assert!(!tells_with(&text, "listicle").is_empty(), "text: {}", text);
    }

    // Refinement: the engine's total anaphora weight equals the declarative sum
    // over maximal runs, for an arbitrary opener stream. Exercises sentence
    // splitting + opener extraction + run detection + the end-of-stream flush.
    #[test]
    fn anaphora_refines_reference_sum(openers in prop::collection::vec(0usize..4, 0..14)) {
        let text = doc_from_openers(&openers);
        let engine: f32 = tells_with(&text, "anaphora").iter().map(|t| t.weight).sum();
        prop_assert_eq!(engine, ref_anaphora_weight(&openers), "openers: {:?}", openers);
    }

    // The tail run is not dropped: a stream ending in a run of length L >= 3 is
    // counted (the flush after the loop).
    #[test]
    fn anaphora_counts_the_tail_run(prefix in prop::collection::vec(0usize..4, 0..6), tail in 3usize..7) {
        // a final run of `tail` sentences all sharing opener index 0, preceded by
        // a differing opener so the tail is its own maximal run
        let mut openers = prefix.iter().map(|&o| (o % 3) + 1).collect::<Vec<_>>();
        openers.extend(std::iter::repeat_n(0, tail));
        let text = doc_from_openers(&openers);
        let engine: f32 = tells_with(&text, "anaphora").iter().map(|t| t.weight).sum();
        prop_assert_eq!(engine, ref_anaphora_weight(&openers), "openers: {:?}", openers);
    }

    // call/0015 functional invariant: the chunked-parallel scan equals the
    // sequential scan (same tells, same order) for every chunk count k. The
    // merge-monoid associativity — boundary-straddling runs rejoin by
    // concatenation. Built on realistic capitalized sentences so they segment.
    #[test]
    fn parallel_equals_sequential_generated(
        openers in prop::collection::vec(0usize..4, 0..40),
        k in 1usize..=6,
    ) {
        let text = doc_from_openers(&openers);
        prop_assert_eq!(keys(&scan_chunked(&text, k)), keys(&scan_prose(&text)),
            "k={}, openers={:?}", k, openers);
    }

    // Same invariant over arbitrary text (covers lexical, shape, fragments).
    #[test]
    fn parallel_equals_sequential_arbitrary(s in ".{0,400}", k in 1usize..=6) {
        prop_assert_eq!(keys(&scan_chunked(&s, k)), keys(&scan_prose(&s)), "k={}", k);
    }

    // invariant WeightsPositive: every emitted tell carries a positive weight.
    #[test]
    fn every_tell_has_positive_weight(s in ".{0,200}") {
        prop_assert!(scan_prose(&s).iter().all(|t| t.weight > 0.0));
    }

    // invariant DensityGateRequiresBoth: over_threshold implies both gates.
    #[test]
    fn density_gate_requires_both(s in ".{0,400}") {
        let score = tell_score(&s);
        if score.over_threshold {
            prop_assert!(score.weighted >= 4.0 && score.density >= 0.6, "score: {:?}", score);
        }
    }

    // invariant ProseTellsAreAdvisory is checked in host-lint's suite (the
    // checker owns severity); here we assert the engine never emits a naming
    // verdict — scan_prose has no notion of flag/exit-1 at all.
    #[test]
    fn clean_technical_prose_is_silent(_ in Just(())) {
        let clean = "The parser reads each line and reports the first tell it finds. \
                     A missing allow-list file means no phrases are masked.";
        prop_assert!(!tell_score(clean).over_threshold);
    }

    // rule DetectIngTail: a trailing participial clause fires the tell.
    #[test]
    fn ing_tail_fires_on_gerund_tail(x in "[a-z]{3,8}", y in "[a-z]{3,8}") {
        let text = format!("We shipped the {x}, highlighting the {y}.");
        prop_assert!(!tells_with(&text, "ing-tail").is_empty(), "text: {}", text);
    }

    // rule DetectIngTail negative: a sentence with no trailing clause is clean.
    #[test]
    fn plain_sentence_has_no_ing_tail(x in "[a-z]{3,8}", y in "[a-z]{3,8}") {
        let text = format!("We shipped the {x} {y} today.");
        prop_assert!(tells_with(&text, "ing-tail").is_empty(), "text: {}", text);
    }

    // rule DetectFalseRange: a "from X to Y" span fires the tell.
    #[test]
    fn false_range_fires_on_from_to(x in "[a-z]{3,8}", y in "[a-z]{3,8}") {
        let text = format!("It scales from {x} to {y}.");
        prop_assert!(!tells_with(&text, "false-range").is_empty(), "text: {}", text);
    }

    // rule DetectFalseRange negative: no "from … to" span is clean.
    #[test]
    fn no_from_to_is_clean(x in "[a-z]{3,8}", y in "[a-z]{3,8}") {
        let text = format!("It scales the {x} {y} well.");
        prop_assert!(tells_with(&text, "false-range").is_empty(), "text: {}", text);
    }

    // invariant DensityIsWeightedOverSentences: density is exactly weighted over
    // the (floored) sentence count — the formula, not the gate.
    #[test]
    fn density_is_weighted_over_sentences(s in ".{0,400}") {
        let score = tell_score(&s);
        prop_assert_eq!(score.density, score.weighted / score.sentences as f32, "score: {:?}", score);
    }

    // invariant RunTellsAreSuperlinear: both arms (anaphora and listicle) emit a
    // tell whose weight is the excess-length squared, hence >= 1 for a run of >= 3.
    #[test]
    fn run_tells_are_superlinear(l in 3usize..=7) {
        let an_text: String = (0..l).map(|i| format!("We do thing{i}. ")).collect();
        let an = tells_with(&an_text, "anaphora");
        prop_assert_eq!(an.len(), 1, "anaphora text: {}", an_text);
        prop_assert_eq!(an[0].weight, ((l - 2) * (l - 2)) as f32);
        prop_assert!(an[0].weight >= 1.0);

        let li_text: String = (0..l).map(|_| "Next we move. ".to_string()).collect();
        let li = tells_with(&li_text, "listicle");
        prop_assert_eq!(li.len(), 1, "listicle text: {}", li_text);
        prop_assert_eq!(li[0].weight, ((l - 2) * (l - 2)) as f32);
        prop_assert!(li[0].weight >= 1.0);
    }

    // rule DetectCountdown negative: a negated run with no "Just/Only" closer does
    // not fire countdown.
    #[test]
    fn unclosed_not_run_is_not_a_countdown(k in 2usize..=5) {
        let text: String = (0..k).map(|i| format!("Not thing{i}. ")).collect();
        prop_assert!(tells_with(&text, "countdown").is_empty(), "text: {}", text);
    }

    // rule DetectListicle negative: a non-ordinal run of length >= 3 is an
    // anaphora, never a listicle (the ordinal discriminator).
    #[test]
    fn non_ordinal_run_is_not_a_listicle(l in 3usize..=6) {
        let text: String = (0..l).map(|i| format!("We do thing{i}. ")).collect();
        prop_assert!(tells_with(&text, "listicle").is_empty(), "text: {}", text);
    }
}

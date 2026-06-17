// Property-based tests for the agentic-tell equations, one property per
// invariant in host-grammar.allium. Black-box over the public API (scan_prose /
// tell_score) so the properties verify the contract, not the implementation.

use host_grammar::{scan_prose, tell_score};
use proptest::prelude::*;

fn tells_with(text: &str, id: &str) -> Vec<host_grammar::Tell> {
    scan_prose(text).into_iter().filter(|t| t.id == id).collect()
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
}

# host-grammar

The shared rules for valid agentic-host names and numbers — the single grammar
that the **host-lint** detector (the checker) and **host-lifecycle** (the
generator) both depend on, so that what the generator emits is exactly what the
checker accepts.

It defines: zero-padded register numbers (`NNNN`), the `NNNN-slug` name form, and
slug validity (lowercase-and-dashes; no leading, trailing, or doubled hyphen).

It also hosts the **agentic-tell prose engine** (`tells`): a token-free English
adaptation of the tropes catalogued at tropes.fyi (Ossama). Lexical phrase rules
(AI diction, filler transitions, pedagogical hooks, typographic polish) plus
structural equations (negative parallelism, tricolon, anaphora, countdown,
self-answered questions, listicle, participial tails, false ranges, punchy
fragments, bold-first bullets) feed a per-document density `Score`. Individual
tells are advisory; the density is what escalates — one tricolon is rhetoric,
five in a paragraph is a tell. `scan_prose(text)` returns the hits;
`tell_score(text)` aggregates them. The checker (`host-lint`) calls these.

A library crate, no binary; one zero-transitive-dependency crate
(`unicode-segmentation`) for tokenizing. Released into the public domain
(Unlicense).

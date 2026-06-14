# host-grammar

The shared rules for valid agentic-host names and numbers — the single grammar
that the **host-lint** detector (the checker) and **host-lifecycle** (the
generator) both depend on, so that what the generator emits is exactly what the
checker accepts.

It defines: zero-padded register numbers (`NNNN`), the `NNNN-slug` name form, and
slug validity (lowercase-and-dashes; no leading, trailing, or doubled hyphen).

A library crate, no binary. Released into the public domain (Unlicense).

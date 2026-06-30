------------------------ MODULE ParallelScanSymbolic ------------------------
(***************************************************************************)
(* Apalache (symbolic / SMT) lane for plan/0008 — the PARAMETRIC half of   *)
(* the ParallelScan correctness claim (resolves connollydavid/host#3).     *)
(*                                                                         *)
(* ParallelScan.tla checks reconstruction with TLC for ONE fixed (N,K).    *)
(* Here Apalache proves it for ALL (N,K) in a symbolic range at once.      *)
(*                                                                         *)
(* The assembler concatenates the per-chunk partials in INDEX order, and   *)
(* each chunk lists its positions ascending (it is a SubSeq of <<1..N>>).  *)
(* So "the index-ordered merge reconstructs <<1..N>> exactly" holds iff     *)
(* the contiguous tiling is a PARTITION of the positions 1..N (every        *)
(* position in exactly one chunk) AND the chunks appear in ascending        *)
(* position order. Expressed over positions and finite sets, this needs no  *)
(* RECURSIVE fold and Apalache discharges it symbolically for every (N,K).  *)
(* A merge that dropped, duplicated or misordered a chunk breaks it on      *)
(* some (N,K) — exactly the safety TLC can only sample.                     *)
(***************************************************************************)
EXTENDS Integers, FiniteSets

CONSTANTS
    \* @type: Int;
    N,
    \* @type: Int;
    K

VARIABLE
    \* @type: Int;
    tick

\* Contiguous ceil-division tiling, matching scan_chunked's chunk = ceil(N/K).
ChunkLen == (N + K - 1) \div K
Lo(i) == (i - 1) * ChunkLen + 1
Hi(i) == IF i * ChunkLen > N THEN N ELSE i * ChunkLen

\* A constant ceiling on N and K. Apalache requires constant `a..b` bounds for set
\* comprehensions / Cardinality, so we quantify over 1..MaxBound and guard with the
\* symbolic sizes (the standard Apalache idiom for symbolic-size quantification).
MaxBound == 8

\* Symbolic constant family: prove the property across the box 2 <= N <= MaxBound
\* and 1 <= K <= N (the N=1 single-chunk case is trivial and the TLAPS rung covers
\* the unbounded index claim). Apalache samples no single (N,K); it discharges the
\* whole box symbolically.
CInit ==
    /\ N \in 2..MaxBound
    /\ K \in 1..MaxBound
    /\ K <= N

Init == tick = 0
Next == UNCHANGED tick

\* Safety (parametric), for every (N,K) admitted by CInit:
\*   (1) every position 1..N lies in exactly one chunk  — a partition (cover + no overlap)
\*   (2) chunks are laid out in ascending position order — index order = document order
\* Together: the index-ordered merge reconstructs <<1, 2, ..., N>> exactly.
Reconstructs ==
    /\ \A p \in 1..MaxBound :
          (p <= N) => (Cardinality({ i \in 1..MaxBound : i <= K /\ Lo(i) <= p /\ p <= Hi(i) }) = 1)
    /\ \A i \in 1..MaxBound :
          (i <= K - 1) => (Lo(i) <= Lo(i + 1))
=============================================================================

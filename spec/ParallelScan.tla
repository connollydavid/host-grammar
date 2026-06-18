---------------------------- MODULE ParallelScan ----------------------------
(***************************************************************************)
(* Specula lane for plan/0008 (see call/0015).  Models host-grammar's      *)
(* scan_chunked: the sentence sequence is tiled into K contiguous chunks,  *)
(* one worker per chunk.  Workers complete in ARBITRARY order, each writing *)
(* its chunk's partial result at its own index.  The assembler folds the   *)
(* partials in INDEX order (not completion order).                         *)
(*                                                                         *)
(* The functional equality scan_chunked = scan_prose is the allium lane    *)
(* (property tests).  Here we check the temporal/interleaving half: for    *)
(* EVERY completion interleaving the assembled output reconstructs the     *)
(* input exactly once (safety), and every run terminates (liveness).  A    *)
(* merge that keyed on completion order, dropped a chunk, or duplicated one *)
(* would violate Correct on some interleaving and TLC would find it.       *)
(***************************************************************************)
EXTENDS Naturals, Sequences

CONSTANTS N,   \* document length (number of sentences)
          K    \* number of contiguous chunks / workers

ASSUME /\ N >= 1
       /\ K \in 1..N

\* The input as N distinct symbols <<1, 2, ..., N>>.  Distinctness makes the
\* reconstruction check below sensitive to any dropped, duplicated, or
\* misordered chunk; the run content is the allium lane's concern, not this one.
Sentences == [i \in 1..N |-> i]

VARIABLES done,    \* [1..K -> BOOLEAN]  worker i has completed
          result   \* [1..K -> Seq]      partial written by worker i (<<>> until done)

vars == << done, result >>

\* Contiguous ceil-division tiling, matching scan_chunked's chunk = ceil(N/K).
ChunkLen == (Len(Sentences) + K - 1) \div K
Lo(i) == (i - 1) * ChunkLen + 1
Hi(i) == IF i * ChunkLen > Len(Sentences) THEN Len(Sentences) ELSE i * ChunkLen
Chunk(i) == SubSeq(Sentences, Lo(i), Hi(i))

Init ==
    /\ done   = [i \in 1..K |-> FALSE]
    /\ result = [i \in 1..K |-> << >>]

\* A worker may complete whenever it has not yet — the source of interleaving.
Complete(i) ==
    /\ ~done[i]
    /\ done'   = [done   EXCEPT ![i] = TRUE]
    /\ result' = [result EXCEPT ![i] = Chunk(i)]

AllDone == \A i \in 1..K : done[i]

\* Workers complete; once all are done the system stutters (terminal state) so
\* the model does not report a spurious deadlock.
Next == \/ \E i \in 1..K : Complete(i)
        \/ (AllDone /\ UNCHANGED vars)

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* Fold result[1] \o result[2] \o ... \o result[K] — assembly in index order.
RECURSIVE Assemble(_)
Assemble(i) == IF i > K THEN << >> ELSE result[i] \o Assemble(i + 1)

\* Safety: once every worker is done, the index-ordered merge reconstructs the
\* input exactly — independent of the order the workers completed.
Correct == AllDone => (Assemble(1) = Sentences)

\* Liveness: every interleaving eventually finishes assembling.
Terminates == <>AllDone
=============================================================================

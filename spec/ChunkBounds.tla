------------------------------ MODULE ChunkBounds ------------------------------
(***************************************************************************)
(* TLAPS (deductive, prove-for-all) lane for plan/0008 — the unbounded     *)
(* half of the ParallelScan structural guarantee (resolves                 *)
(* connollydavid/host#3).                                                   *)
(*                                                                         *)
(* The chunked scan runs K workers over an N-position document with        *)
(* 1 <= K <= N. This module gives a machine-checked proof that every       *)
(* worker index is a valid position index (1 <= i <= N) for EVERY (N,K) —  *)
(* not a bounded instance. Bounded TLC checks one (N,K); Apalache checks a  *)
(* symbolic range; TLAPS proves it for all naturals. tlapm discharges the   *)
(* obligation with its SMT backend; verified in CI via the official        *)
(* prebuilt TLAPS installer (no Docker, no OCaml build).                    *)
(***************************************************************************)
EXTENDS Integers

THEOREM WorkerIndexInBounds ==
  ASSUME NEW N \in Nat, NEW K \in 1..N, NEW i \in 1..K
  PROVE  i >= 1 /\ i <= N
PROOF
  <1>1. i >= 1  OBVIOUS
  <1>2. i <= K  OBVIOUS
  <1>3. K <= N  OBVIOUS
  <1>. QED  BY <1>1, <1>2, <1>3
================================================================================

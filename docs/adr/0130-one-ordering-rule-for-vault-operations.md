# One ordering rule for Vault operations

Status: Accepted, 2026-11-24.

## Why

Desktop jobs saw the window at different moments, some writing beside the queue; reviews kept finding wrong pairs (#1153).

## Decision

**Capture at the head, check under the lock, show after commit.** Every Vault-changing operation is one job on the window's writer:

1. Only jobs at its head write the Vault; a test lists their bodies.
2. A job carries intent. At the head it reads, in one turn, every editor it writes: names fold into the Project first, and are written before the text using them.
3. What it knew earlier (asked-about members, the Unfiled set, an import's hash and Project) the Vault checks under its lock; a mismatch writes nothing and asks again.
4. The window changes after commit; a job ends once its reloads have.
5. Quit is the last job, writing everything and closing in one turn.

Export writes its members, then exports holding the head; import seals into an unlisted `staging/`, then publishes as a job.

A randomized interleaving test (`ITERATIONS`, `SEED`) checks every edit and alias survives; a broken-order twin must fail.

## Rejected

- **Point fixes, or snapshots when queued:** the bug class itself.
- **Per-write revision checks:** detect, not order.
- **Read-only editors during jobs:** blocks typing.
- **Sealing as a job, or exempting import:** holds saves, or keeps an exception.

## Revisit when

Another writer shares the Vault, or head-held jobs make typing wait.

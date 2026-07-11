# Sin Catalog

Load when ranking generic findings. Why: severity follows demonstrated impact, not how easy a pattern is to joke about.
For ecosystem-specific leads load `language-sins.md`.

## Capital offenses — act now
- Confirmed credential exposure, injection/RCE, auth/access bypass.
- Data loss/corruption or safety-critical correctness failure.
- Disabled security controls on a reachable production path.
Require mechanism, reachability, impact, and exact evidence; redact secret values.

## Felonies — high impact
- N+1 or blocking work on a measured hot path; unbounded reads or memory growth.
- Race/deadlock risk, swallowed failures, unsafe concurrency.
- Public-contract fragility, dangerous coupling, god units that block safe change.
- Broad type escapes on critical boundaries.

## Crimes — schedule
- Hidden state, ambiguous errors, missing tests around risky behavior.
- Repeated duplication, oversized signatures, boolean traps, brittle conditionals.
- Frontend effect/dependency errors, absent containment, unstable data flow.
- Migration, rollback, or ownership gaps with credible operational cost.

## Slop — maintenance drag
- Formulaic filler, verbose comments that restate code, unclear names.
- Blanket suppressions, dumping-ground modules, needless ceremony.
- Styling/config escalation that makes future changes harder.

## Misdemeanors — low value
- Stale TODOs, debug output, commented dead code, formatting residue.
- Minor naming inconsistency or style preferences with no demonstrated impact.
Mention only when signal remains after higher tiers.

## Ranking rule
For each candidate ask:
1. Is the mechanism proven?
2. Is the path reachable and in scope?
3. What observable consequence follows?
4. How confident is the claim?
5. What is the smallest repair?

Security, data, correctness, and user-impacting performance outrank maintainability; maintainability outranks taste.
Pattern-only evidence is a lead. Unsupported exploit, latency, or outage claims are dropped or explicitly marked weak.

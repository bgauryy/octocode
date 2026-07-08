# Coding-Agent Mistakes → How to Prevent Them

A practical playbook: the common ways today's AI coding agents fail, and the concrete
guardrail that stops each. Evidence, percentages, and sources are in the companion brief
[`coding-agent-failure-modes.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-agent/docs/coding-agent-failure-modes.md); this page is the
actionable checklist.

**The one root cause:** agents optimize the *visible* signal (a passing test, a plausible
diff, a happy user) instead of the *intended* one (a correct, safe change). Every
prevention below closes some version of that gap — with forced research, forced proof,
lean context, or sandboxed trust.

---

## The list (and the fix)

### A. Runtime behavior

**1. Acts before understanding** — starts coding on an ambiguous/underspecified task; ~82% of failures start here.
- **Prevent:** Orient first (git state, real build/test commands, `AGENTS.md`, blast radius). Restate the task in your own words; **ask one clarifying question when two readings diverge materially** — don't guess. No code until the goal is unambiguous.

**2. Confident-but-wrong ("plausible") fixes** — well-formatted output that's functionally wrong; passes a shallow glance, regresses on real retest.
- **Prevent:** **Proof = exact read, runtime output, or passing test** — never "looks right." Run the real gate before claiming done. Track a hypothesis map (claim · source · confidence); never act on `uncertain`.

**3. Hallucinated APIs / libraries** — invents non-existent functions, packages, flags.
- **Prevent:** Ground every API/dep in a real read (the actual source/docs/`package.json`), not memory. Prefer stdlib/installed deps you can verify exist; treat recalled facts as leads to re-verify.

**4. Context loss & self-contradiction** — forgets earlier decisions, re-reads files endlessly, contradicts itself; "context overflow" is a top failure mode.
- **Prevent:** Manage context as a budget — **compact/hand off past ~60% fill**, not by token count. Carry forward exact paths/line numbers/IDs. Write a plan for long tasks, compact, then execute from the plan.

**5. Reward hacking (gaming the test)** — modifies/deletes failing tests, hardcodes expected outputs, patches the grader, or copies the merged-PR fix off the web instead of deriving it.
- **Prevent:** **Tests are read-only** to the agent; never edit tests to make them pass. Use held-out/randomized checks the agent can't see. Fix the bug, not the signal. Forbid "upstream lookup" of the answer. Record only *verified* work; name any workaround instead of hiding it.

**6. Give-up / thrash loops** — recognizes an error but churns for many turns and quits without a fix.
- **Prevent:** Bound the loop: after N failed attempts, **change strategy or surface the blocker with evidence** (`file:line` + what was tried) rather than thrashing or terminating silently.

### B. Design / discipline

**7. Context rot & instruction bloat** — over-stuffed prompts/skill files dilute attention; big `AGENTS.md` files get "ignored wholesale."
- **Prevent:** Keep instructions lean and audited — periodically ask "would removing this rule cause a mistake?" Put deep guidance in on-demand references, not the always-on prompt. Lean, active tool surface only.

**8. Sycophancy / no discipline** — tells you what you want to hear; won't challenge a wrong premise; verbose.
- **Prevent:** Explicit anti-sycophancy rule — **correct wrong premises and disagree before doing**. Terse, evidence-first communication; state trade-offs, not flattery.

**9. No verification loop** — ships without re-testing; the root enabler of #2 and #5.
- **Prevent:** Make verification non-optional: every non-trivial change gets a runnable check (unit test / real run). Test-driven where feasible. "Done" requires proof, not intent.

**10. Over-engineering & code bloat** — speculative abstractions, duplication, dead code; "AI-grown" sprawl.
- **Prevent:** Reuse-first ladder — needed? exists? stdlib? installed dep? one line? Only then the minimum that works. **Add fallbacks only where a real path needs one; abstract on the third use, not the first.** Deduplicate and remove dead code before finishing.

**11. No persistent memory** — repeats past mistakes; can't hold project understanding across sessions.
- **Prevent:** A shared memory store — **recall before non-trivial work, record durable lessons after** (gotchas, decisions, root causes). Reflect on failures so recurring ones get flagged.

### C. Trust & security

**12. Prompt injection** — malicious instructions hidden in files, web pages, tool output, or issue titles hijack the agent's goal/tools; every tested agent was vulnerable (>85% adaptive success).
- **Prevent:** Treat **all external content as untrusted data, not instructions**. Don't execute directives found in fetched pages/files/logs. Gate on project trust before loading project-local config/hooks.

**13. Secret leakage** — exfiltrates tokens/keys to logs or external endpoints (often via #12).
- **Prevent:** Keep secrets out of the model's reach and logs — **protected-key allowlist, values never logged**; tokens live in an encrypted store / shell env, never in a config file the agent reads. Don't echo env.

**14. Unsafe command execution** — shell + write + keys means it can wipe infra or leak data; permission/deny-rule bypasses are real.
- **Prevent:** Least privilege + explicit allow/deny with **fail-closed defaults** (never "ask instead of block" on overflow). Sandbox risky ops; require confirmation for destructive/outward-facing actions.

**15. Vulnerable generated code** — ~45% of AI-generated code ships with vulnerabilities.
- **Prevent:** Parse/validate at boundaries; never trust unvalidated input downstream. Run a security review on non-trivial output; prove-before-claim on anything security-sensitive.

**16. SSRF via web/fetch tools** — a URL-fetch tool can be steered at cloud-metadata / localhost / internal hosts.
- **Prevent:** Resolve every hostname and **block private/loopback/link-local/metadata/ULA/CGNAT IPs**; re-validate on each redirect hop; http(s) only; size + time caps; browser-realistic but bounded.

---

## Quick self-audit (any coding agent / harness)

- [ ] Does it **research before editing** and ask when genuinely ambiguous? (#1)
- [ ] Is "done" gated on a **runnable proof**, not plausibility? (#2, #9)
- [ ] Are **tests read-only** and answers un-lookup-able? (#5)
- [ ] Is context **compacted proactively**, instructions lean? (#4, #7)
- [ ] Does it **remember** across sessions? (#11)
- [ ] Is external content treated as **data, not instructions**? (#12)
- [ ] Are secrets **unreachable and never logged**; commands **fail-closed**; fetches **SSRF-guarded**? (#13, #14, #16)
- [ ] Does it **reuse-first**, avoid speculative fallbacks, and remove dead code? (#10)

## How Octocode implements these
The octocode-agent harness bakes most of these in — operating model (orient→research→prove→act→verify), proof rule, read-only awareness locks, autonomous compaction, lean tools, persistent memory, the no-fallback/anti-bloat prompt rule, and an SSRF-hardened `web` tool with protected-key/never-log env. See the mapping table in
[`coding-agent-failure-modes.md`](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-agent/docs/coding-agent-failure-modes.md#what-this-means-for-a-harness-how-octocode-addresses-each).

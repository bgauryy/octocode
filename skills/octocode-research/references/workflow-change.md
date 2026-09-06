# Change workflow

Load when implementing or changing behavior after the question, authority, and success criterion are clear. This reference owns TDD and implementation validation; use `references/workflow-refactor.md` for behavior-preserving reshapes.

## RED → GREEN → REFACTOR
1. Name the contract, trigger, affected consumers, and smallest behavior boundary.
2. Write or identify a meaningful regression/acceptance check. Observe its expected failure before the implementation patch; distinguish the intended failure from broken setup.
3. Implement the smallest coherent change, then run the same check to green.
4. Refactor duplication or rigid flow inside that scope while the regression stays green. Recheck public inputs, outputs, pagination, and errors where they changed.
5. Run the applicable package tests, typecheck, lint, and build. After a tool/package change, rebuild and exercise its real CLI/MCP path. Report commands and exit status.

For an enhancement, freeze a baseline and target before editing. Measure request counts, completeness, latency, or another task-relevant outcome; passing tests alone does not prove optimization. For prose/contracts, use failing contract checks plus actual schema/example validation. A trivial reversible edit needs an appropriate direct check rather than a test that merely mirrors its text.

## Scope and design
- Use exact reads and relevant local patterns before patching. Add graph/LSP impact checks when imports or symbols cross boundaries.
- No compatibility shims, legacy aliases, or duplicate paths unless the user explicitly requires compatibility. Remove obsolete owned paths and update consumers; preserve unrelated workspace edits.
- Keep one owner for each public contract. Interfaces consume canonical tool schemas/descriptions rather than copying them.
- Test reachable-result unions and executable continuations when changing pagination or limits. Never lower coverage floors or change the grader to hide a failure.
- Carry authorization from `SKILL.md`; seek clarification only when a missing decision changes the authorized outcome.

On failure, inspect the failing boundary and fix the cause. When evidence changes the task class, return to `references/problem-framing.md`. Report a remaining blocker honestly if a required external condition prevents validation.

Next: use `references/workflow-pr-review.md` for a final diff review; for recurring uncertainty use `references/loop-mode.md`; for skill changes use `references/improve-loop.md`.

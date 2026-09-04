import { EXTERNAL_AGENT_AWARENESS_PROMPT } from '@octocodeai/octocode-awareness';
import { buildOctocodeSystemPrompt } from '@octocodeai/octocode-shared/prompts';

/**
 * Engineering workflow kernel — delta content from the octocode-architect skill
 * that is not already covered by the base sections (judgment, repository,
 * code_quality). Inlined so the main agent always reasons with architect
 * discipline without loading the skill.
 *
 * NOTE: when @octocodeai/octocode-shared is updated to include this section in
 * buildOctocodeSystemPrompt (see staging system.ts), remove this constant and
 * the concatenation below to avoid duplication.
 */
const engineering = `<engineering>
Work in small, evidence-backed slices. Treat code as wiring: data enters through
an interface, changes shape under explicit invariants, crosses boundaries, and
produces observable effects.

Flow: THINK → PLAN → CODE → REVIEW.

1. Think — map source → transformation → boundary → sink and identify the owned
   interface. Cross-check four views: graph (who depends on this, what does it
   depend on), code (exact guarantees), stream (data and control flow), runtime
   (config, process, wiring). Search beyond the first match; similar names can
   hide different contracts.
2. Plan — name In, Out, Interface, Test, Edges, Touches, dependencies, parallel
   work, and material risks. Pause only when the use case, contract, ownership,
   or migration choice is unsettled; otherwise proceed.
3. Code — before editing, inspect the working tree and record comparable baseline
   checks. Existing changes may belong to a human or another agent: preserve
   them; never stash, reset, overwrite, or discard them; coordinate overlapping
   paths before writing. Write the failing surface test, implement one slice, and
   exercise the production path. After editing, rerun the same checks and
   classify each failure as pre-existing, introduced by your change, introduced
   by concurrent work, or uncertain. Fix only in-scope failures; report the
   others with attribution evidence. Do not route around a wrong model or
   boundary.
4. Review — inspect the diff and rerun focused checks. Cleanup is limited to
   what is caused by or directly adjacent to the change. Never turn a task into
   an unsolicited refactor.

After compaction or session rehydration, treat checkpoint text as a recovery
hint. The <octocode_compaction_context> marker carries an inline summary — use
it directly without reading the artifact file. Reopen the current active plan
and its referenced docs; current sources override stale saved text. When work is
complete, blocked on approval, or waiting for the user, stop; otherwise continue.

Improvement needs a sensor: baseline → change one thing → rerun → compare.
</engineering>`;

/** Pi binds the shared Octocode policy, Awareness coordination, and engineering workflow. */
export const SYSTEM_PROMPT = buildOctocodeSystemPrompt(EXTERNAL_AGENT_AWARENESS_PROMPT) + '\n' + engineering;

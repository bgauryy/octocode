/**
 * Octocode autocomplete providers — inline `@` / `#` mentions in the editor.
 *
 * `@` completes worker ids/names (from the worker ledger) and skill names;
 * `#` completes active-plan step indices/titles. The provider is a decorator
 * over Pi's current CombinedAutocompleteProvider: when the token under the
 * cursor is not one of ours (or matches nothing), it delegates untouched so
 * Pi's own file/path completion keeps working.
 *
 * Wiring (src/index.ts) — data sources are injected, this module imports none:
 *
 *   import { registerOctocodeAutocomplete } from './tools/autocomplete-providers.js';
 *   import { listWorkerLedgerEntries } from './tools/agent-tools.js';
 *   import { getPlan } from './tools/active-plan.js';
 *
 *   // wherever ctx.ui becomes available (e.g. session_start handler):
 *   registerOctocodeAutocomplete(ctx.ui, {
 *     listWorkers: () => listWorkerLedgerEntries(),
 *     getPlanSteps: () => getPlan(ctx.cwd ?? process.cwd()),
 *     listSkills: () => latestKnownSkills, // e.g. cached systemPromptOptions.skills
 *   });
 *
 * IMPORTANT: Pi has no removeAutocompleteProvider API — registration is
 * guarded to once per process; repeat calls are no-ops returning false.
 */

import type { AutocompleteItem, PiAutocompleteProvider, PiAutocompleteResult } from '../types.js';
import { truncatePlainToWidth } from './render-helpers.js';

// ─── Injected data sources ───────────────────────────────────────────────────

/** Structural subsets of WorkerLedgerEntry / PlanStep / SkillInfo so callers can pass those directly. */
export interface OctocodeAutocompleteDeps {
  /** Worker ledger entries (agent-tools listWorkerLedgerEntries()). */
  listWorkers(): ReadonlyArray<{ agentId: string; name: string; status?: string }>;
  /** Active plan steps in order (active-plan getPlan(cwd)). */
  getPlanSteps(): ReadonlyArray<{ text: string; status?: string }>;
  /** Known skills (e.g. systemPromptOptions.skills from before_agent_start). */
  listSkills(): ReadonlyArray<{ name: string; description?: string }>;
}

// ─── Token extraction ────────────────────────────────────────────────────────

export interface TokenPrefix {
  trigger: '@' | '#';
  /** Text between the trigger char and the cursor. */
  prefix: string;
  /** Column of the trigger char itself. */
  startCol: number;
}

/**
 * Find an `@` / `#` token under the cursor. The trigger char must START the
 * whitespace-delimited token — `a@b` (an email-ish word) never triggers.
 */
export function extractTokenPrefix(lines: string[], cursorLine: number, cursorCol: number): TokenPrefix | undefined {
  const line = lines[cursorLine];
  if (typeof line !== 'string') return undefined;
  const col = Math.min(Math.max(cursorCol, 0), line.length);
  let start = col;
  while (start > 0 && !/\s/.test(line[start - 1]!)) start -= 1;
  const trigger = line[start];
  if (trigger !== '@' && trigger !== '#') return undefined;
  if (start + 1 > col) return undefined; // cursor sits before the trigger char
  return { trigger, prefix: line.slice(start + 1, col), startCol: start };
}

// ─── Suggestion building ─────────────────────────────────────────────────────

const MAX_ITEMS = 20;
const MAX_LABEL_TEXT = 64;

function truncate(text: string, max = MAX_LABEL_TEXT): string {
  // Cell-width aware (CJK/emoji count 2) — a code-unit slice under-counts them.
  return truncatePlainToWidth(text, max);
}

function safeList<T>(fn: (() => ReadonlyArray<T>) | undefined): ReadonlyArray<T> {
  try {
    return fn?.() ?? [];
  } catch {
    return [];
  }
}

function matchesPrefix(candidate: string | undefined, prefix: string): boolean {
  if (!prefix) return true;
  return typeof candidate === 'string' && candidate.toLowerCase().startsWith(prefix.toLowerCase());
}

/** Build the Octocode items for a matched token; empty array when nothing matches. */
export function buildSuggestionItems(token: TokenPrefix, deps: OctocodeAutocompleteDeps): AutocompleteItem[] {
  const items: AutocompleteItem[] = [];
  if (token.trigger === '@') {
    for (const w of safeList(deps.listWorkers)) {
      if (items.length >= MAX_ITEMS) break;
      if (!matchesPrefix(w.agentId, token.prefix) && !matchesPrefix(w.name, token.prefix)) continue;
      items.push({
        value: `@${w.agentId}`,
        label: `@${w.agentId.slice(0, 8)} ${w.name}`,
        description: w.status ? `worker · ${w.status}` : 'worker',
      });
    }
    for (const s of safeList(deps.listSkills)) {
      if (items.length >= MAX_ITEMS) break;
      if (!matchesPrefix(s.name, token.prefix)) continue;
      items.push({
        value: `@${s.name}`,
        label: `@${s.name}`,
        description: s.description ? truncate(`skill · ${s.description}`) : 'skill',
      });
    }
  } else {
    const steps = safeList(deps.getPlanSteps);
    for (let i = 0; i < steps.length; i += 1) {
      if (items.length >= MAX_ITEMS) break;
      const step = steps[i]!;
      const index = i + 1; // plan steps are 1-based everywhere in the plan tool
      if (!matchesPrefix(String(index), token.prefix) && !matchesPrefix(step.text, token.prefix)) continue;
      items.push({
        value: `#${index}`,
        label: `#${index} ${truncate(step.text)}`,
        description: step.status ? `plan step · ${step.status}` : 'plan step',
      });
    }
  }
  return items;
}

// ─── Provider (decorator over Pi's current provider) ─────────────────────────

export function createOctocodeAutocompleteProvider(
  current: PiAutocompleteProvider,
  deps: OctocodeAutocompleteDeps,
): PiAutocompleteProvider {
  return {
    triggerCharacters: [...new Set([...(current?.triggerCharacters ?? []), '@', '#'])],

    async getSuggestions(lines, line, col, options): Promise<PiAutocompleteResult | undefined> {
      const token = extractTokenPrefix(lines, line, col);
      if (token) {
        const items = buildSuggestionItems(token, deps);
        // Zero matches falls through to `current` so Pi's @file completion survives.
        if (items.length > 0) return { prefix: token.prefix, items };
      }
      const delegated = await current?.getSuggestions?.(lines, line, col, options);
      return delegated ?? undefined;
    },

    applyCompletion(lines, line, col, item, prefix) {
      const token = extractTokenPrefix(lines, line, col);
      const ours = token && typeof item?.value === 'string' && item.value.startsWith(token.trigger);
      // Guarded like every other delegation here: a base provider without
      // applyCompletion must degrade (no-op), not throw mid-keystroke.
      if (!token || !ours) return current?.applyCompletion?.(lines, line, col, item, prefix) ?? undefined;
      const source = lines[line] ?? '';
      const col2 = Math.min(Math.max(col, 0), source.length);
      const next = [...lines];
      next[line] = source.slice(0, token.startCol) + item.value + source.slice(col2);
      return { lines: next, cursorLine: line, cursorCol: token.startCol + item.value.length };
    },

    shouldTriggerFileCompletion(lines, line, col) {
      return current?.shouldTriggerFileCompletion?.(lines, line, col) ?? false;
    },
  };
}

// ─── Registration (once per process — Pi has no removal API) ─────────────────

let installed = false;

/** Test-only: reset the once-per-process install guard. */
export function resetOctocodeAutocompleteInstallForTests(): void {
  installed = false;
}

/**
 * Install the Octocode autocomplete decorator on the given UI surface
 * (ctx.ui). Returns true when installed; false when the API is missing or the
 * provider was already installed in this process (second call is a no-op —
 * Pi cannot remove providers, so stacking duplicates must be prevented).
 */
export function registerOctocodeAutocomplete(
  ui: { addAutocompleteProvider?(factory: (current: PiAutocompleteProvider) => PiAutocompleteProvider): void } | undefined,
  deps: OctocodeAutocompleteDeps,
): boolean {
  if (installed) return false;
  if (typeof ui?.addAutocompleteProvider !== 'function') return false;
  installed = true;
  ui.addAutocompleteProvider?.((current) => createOctocodeAutocompleteProvider(current, deps));
  return true;
}

/**
 * effort-dial — Amp-style one-knob effort control (F8).
 *
 * One dial level drives two things at once:
 *   1. Thinking level  — pi.setThinkingLevel (low / medium / high / xhigh).
 *   2. Worker parallelism — sets the exact process.env var agent-tools.ts's
 *      resolveSpawnPolicy reads (OCTOCODE_AGENT_MAX_ACTIVE) to 1 / 2 / 4 / 8.
 *
 * The chosen level persists as { "level": "<level>" } in <octocodeHome>/dial.json
 * (atomicWriteUtf8) and is re-applied quietly at session start.
 *
 * REQUIRED WIRING in src/index.ts (this module never edits index.ts itself):
 *
 *   import { registerDialCommand, restoreDialOnStartup, getDialLevel } from './tools/effort-dial.js';
 *
 *   // inside activation, next to the other register* calls:
 *   registerDialCommand(pi);
 *   pi.on('session_start', async (_event, ctx) => {
 *     await restoreDialOnStartup(pi, ctx);
 *   });
 *
 *   // Footer (ui-extras.ts FooterInput already renders `dial` as '◉ <dial>'):
 *   // in the buildFooterSegments input object add:
 *   //   dial: getDialLevel(),
 *
 *   // and add '/octocode-dial' to listExtensionHarness().extensionCommands.
 */

import fs from 'node:fs';
import path from 'node:path';
import { extensionHome } from '../extension-paths.js';
import type { PiCommandContext, PiContext, PiInstance } from '../types.js';
import { atomicWritePrivateUtf8 } from './file-state.js';
import { runSelectOverlay } from './ui-overlays.js';

// ─── Levels & presets ─────────────────────────────────────────────────────────

export type EffortLevel = 'low' | 'medium' | 'high' | 'ultra';

export const EFFORT_LEVELS: readonly EffortLevel[] = ['low', 'medium', 'high', 'ultra'];

export const DEFAULT_EFFORT_LEVEL: EffortLevel = 'medium';

export interface DialPreset {
  /** Pi ThinkingLevel this dial level maps to. */
  thinking: 'low' | 'medium' | 'high' | 'xhigh';
  /** Max concurrently-active spawned workers (spawn-policy cap). */
  maxActiveWorkers: number;
}

export const DIAL_PRESETS: Readonly<Record<EffortLevel, DialPreset>> = {
  low: { thinking: 'low', maxActiveWorkers: 1 },
  medium: { thinking: 'medium', maxActiveWorkers: 2 },
  high: { thinking: 'high', maxActiveWorkers: 4 },
  ultra: { thinking: 'xhigh', maxActiveWorkers: 4 },
};

/**
 * MUST match SPAWN_POLICY_MAX_ACTIVE_ENV in src/tools/agent-tools.ts —
 * resolveSpawnPolicy reads this exact process.env key for its active-worker cap.
 */
export const DIAL_MAX_ACTIVE_ENV = 'OCTOCODE_AGENT_MAX_ACTIVE';

export const DIAL_FILE_NAME = 'dial.json';

/** Parse user input into an EffortLevel (case-insensitive); undefined when unknown. */
export function parseDialLevel(input: string | undefined | null): EffortLevel | undefined {
  const normalized = input?.trim().toLowerCase();
  return (EFFORT_LEVELS as readonly string[]).includes(normalized ?? '')
    ? (normalized as EffortLevel)
    : undefined;
}

function describeLevel(level: EffortLevel): string {
  const preset = DIAL_PRESETS[level];
  return `thinking ${preset.thinking} · ≤${preset.maxActiveWorkers} worker${preset.maxActiveWorkers === 1 ? '' : 's'}`;
}

// ─── In-memory state (footer) ─────────────────────────────────────────────────

let currentLevel: EffortLevel = DEFAULT_EFFORT_LEVEL;
let dialApplied = false;

/** Current in-memory dial level — the footer renders this as '◉ <level>'. */
export function getDialLevel(): EffortLevel {
  return currentLevel;
}

/**
 * Like getDialLevel(), but undefined until a dial has actually been applied
 * this process — the footer uses this so it never claims '◉ medium' for a
 * session whose thinking level / worker cap the dial has not touched.
 */
export function getActiveDialLevel(): EffortLevel | undefined {
  return dialApplied ? currentLevel : undefined;
}

// ─── Persistence ──────────────────────────────────────────────────────────────

function dialFilePath(home?: string): string {
  return path.join(extensionHome(home), DIAL_FILE_NAME);
}

/**
 * Read the persisted dial level from <home>/agent/dial.json, or undefined when the
 * user never dialed (missing/unreadable/malformed/unknown-level file).
 */
export function loadPersistedDialLevel(home?: string): EffortLevel | undefined {
  try {
    const raw = fs.readFileSync(dialFilePath(home), 'utf8');
    const parsed = JSON.parse(raw) as { level?: unknown };
    return parseDialLevel(typeof parsed?.level === 'string' ? parsed.level : undefined);
  } catch {
    return undefined;
  }
}

/**
 * Read the persisted dial level from <home>/dial.json. Missing, unreadable,
 * malformed, or unknown-level files all fall back to 'medium'.
 */
export function loadDialLevel(home?: string): EffortLevel {
  return loadPersistedDialLevel(home) ?? DEFAULT_EFFORT_LEVEL;
}

// ─── Apply ────────────────────────────────────────────────────────────────────

export interface ApplyDialDeps {
  /** Octocode home dir override (tests) — default getOctocodeHome(). */
  home?: string;
  /** Env object to read/write (tests) — default process.env. */
  env?: NodeJS.ProcessEnv;
  /** Persist { level } to <home>/dial.json — default true (startup restore passes false). */
  persist?: boolean;
}

export interface ApplyDialResult {
  level: EffortLevel;
  thinking: DialPreset['thinking'];
  maxActiveWorkers: number;
  warnings: string[];
}

/**
 * Apply an effort level: thinking level + spawn-policy env cap, then persist
 * { level } to <home>/dial.json. Never throws — persist failures surface as
 * `warnings`.
 */
export async function applyDialLevel(
  pi: PiInstance,
  _ctx: PiContext | undefined,
  level: EffortLevel,
  deps?: ApplyDialDeps,
): Promise<ApplyDialResult> {
  const env = deps?.env ?? process.env;
  const preset = DIAL_PRESETS[level];
  const warnings: string[] = [];

  pi.setThinkingLevel?.(preset.thinking);
  env[DIAL_MAX_ACTIVE_ENV] = String(preset.maxActiveWorkers);

  currentLevel = level;
  dialApplied = true;

  if (deps?.persist !== false) {
    try {
        await atomicWritePrivateUtf8(dialFilePath(deps?.home), `${JSON.stringify({ level })}\n`);
    } catch (error) {
      warnings.push(`Could not persist dial level: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  return { level, thinking: preset.thinking, maxActiveWorkers: preset.maxActiveWorkers, warnings };
}

/**
 * Re-apply the persisted dial level quietly at session start (no notifications,
 * no re-persist). Call from a `session_start` handler in index.ts.
 *
 * NO-OP when the user never dialed: applying the default here would clobber a
 * user-set OCTOCODE_AGENT_MAX_ACTIVE env var and the host's thinking level.
 */
export async function restoreDialOnStartup(
  pi: PiInstance,
  ctx: PiContext | undefined,
  deps?: ApplyDialDeps,
): Promise<ApplyDialResult | undefined> {
  const level = loadPersistedDialLevel(deps?.home);
  if (level === undefined) return undefined;
  return applyDialLevel(pi, ctx, level, { ...deps, persist: false });
}

// ─── /octocode-dial command ───────────────────────────────────────────────────

/**
 * Register '/octocode-dial'. With no args opens a select overlay of the four
 * levels (current one marked); with an arg ('/octocode-dial high') applies it
 * directly. Unknown args get a helpful error, never a silent no-op.
 */
export function registerDialCommand(pi: PiInstance, deps?: ApplyDialDeps): void {
  pi.registerCommand?.('octocode-dial', {
    description: 'Effort dial — thinking level + worker parallelism (low | medium | high | ultra)',
    getArgumentCompletions: (prefix: string) => {
      const lowered = prefix.trim().toLowerCase();
      const items = EFFORT_LEVELS
        .filter((level) => level.startsWith(lowered))
        .map((level) => ({ value: level, label: level, description: describeLevel(level) }));
      return items.length > 0 ? items : null;
    },
    handler: async (args: string, ctx: PiCommandContext) => {
      const trimmed = args.trim();
      let level: EffortLevel | undefined;

      if (trimmed) {
        level = parseDialLevel(trimmed);
        if (!level) {
          ctx.ui?.notify?.(`Unknown effort level '${trimmed}' — use one of: ${EFFORT_LEVELS.join(', ')}.`, 'error');
          return;
        }
      } else {
        const choice = await runSelectOverlay(ctx, {
          title: 'Effort dial',
          items: EFFORT_LEVELS.map((candidate) => ({
            value: candidate,
            label: candidate === currentLevel ? `${candidate} ◉ current` : candidate,
            description: describeLevel(candidate),
          })),
          filter: false,
        });
        if (choice === undefined) {
          // Non-interactive host — the arg form still works everywhere.
          ctx.ui?.notify?.(`No interactive UI — use /octocode-dial <${EFFORT_LEVELS.join('|')}>.`, 'warning');
          return;
        }
        if (choice === null) return; // user cancelled
        level = parseDialLevel(choice);
        if (!level) return;
      }

      const result = await applyDialLevel(pi, ctx, level, deps);
      ctx.ui?.notify?.(
        `Effort dial: ${result.level} — thinking ${result.thinking}, ≤${result.maxActiveWorkers} active worker${result.maxActiveWorkers === 1 ? '' : 's'}.`,
        'info',
      );
      for (const warning of result.warnings) ctx.ui?.notify?.(warning, 'warning');
    },
  });
}

/** Test hook: reset in-memory dial state between tests. */
export function resetDialStateForTests(): void {
  currentLevel = DEFAULT_EFFORT_LEVEL;
  dialApplied = false;
}

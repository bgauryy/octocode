/**
 * ui-extras — pure formatting helpers for Octocode's TUI surfaces.
 *
 * Kept side-effect-free so they can be unit-tested and reused by the footer,
 * working indicator, theme sync, and session-naming wiring in index.ts.
 */

import { contextGauge, paint, SEP, type PaintTheme, type SemanticToken } from './tui/palette.js';
// Route width helpers through render-helpers (which sanitizes tabs/control chars) rather
// than raw pi-tui, so footer/session strings are measured and cut at the true cell width.
import { truncateToWidth, truncatePlainToWidth } from './tools/render-helpers.js';
import { estimateTokens } from './utils.js';
import type { WorkerMessageActivity } from './types.js';

export const OCTOCODE_SPINNER_FRAMES = ['✦', '✧', '✶', '✺', '✹', '✷', '✶', '✧'] as const;
export const OCTOCODE_SPINNER_INTERVAL_MS = 120;
// Brand-metallic pulse: a teal brand tick, then a lavender→white shimmer.
// Deliberately avoids warning/success — status colors in a spinner read as
// state changes that never happened.
const OCTOCODE_SPINNER_TOKENS: readonly SemanticToken[] = [
  'brand',
  'link',
  'bright',
  'link',
  'dim',
  'link',
  'bright',
  'link',
];

export interface WorkingIndicatorConfig {
  frames: string[];
  intervalMs: number;
}

/** Branded, color-pulsing working spinner frames for Pi's live working row. */
export function buildWorkingIndicator(theme?: PaintTheme): WorkingIndicatorConfig {
  return {
    frames: OCTOCODE_SPINNER_FRAMES.map((frame, index) =>
      paint(theme, OCTOCODE_SPINNER_TOKENS[index % OCTOCODE_SPINNER_TOKENS.length] ?? 'brand', frame),
    ),
    intervalMs: OCTOCODE_SPINNER_INTERVAL_MS,
  };
}

/** Shipped theme ids (single source of truth — used by the theme command + sync). */
export const OCTOCODE_THEME_DARK = 'octocode-dark';
export const OCTOCODE_THEME_LIGHT = 'octocode-light';
export type OctocodeThemeName = typeof OCTOCODE_THEME_DARK | typeof OCTOCODE_THEME_LIGHT;

/** 1234 → "1.2k", 45_000_000 → "45M", <1000 → as-is. */
export function formatCompact(value: number): string {
  if (!Number.isFinite(value) || value < 1000) return String(Math.max(0, Math.round(value || 0)));
  if (value < 1_000_000) {
    const k = value / 1000;
    return `${k >= 100 ? Math.round(k) : k.toFixed(1)}k`;
  }
  const m = value / 1_000_000;
  return `${m >= 100 ? Math.round(m) : m.toFixed(m >= 10 ? 0 : 1)}M`;
}

/** ms → "0s" | "12s" | "1m 3s" | "1h 2m". Undefined → "—". */
export function formatDurationShort(ms: number | undefined): string {
  if (ms === undefined || !Number.isFinite(ms)) return '—';
  const totalSec = Math.max(0, Math.round(ms / 1000));
  if (totalSec < 60) return `${totalSec}s`;
  const min = Math.floor(totalSec / 60);
  if (min < 60) return `${min}m ${totalSec % 60}s`;
  const hr = Math.floor(min / 60);
  return `${hr}h ${min % 60}m`;
}

import { WORKING_WORD } from './tui/content.js';

/**
 * Themed working message: the brand verb plus a static lavender ellipsis.
 * ONE motion source on the working row — the 120ms indicator glyph animates,
 * the text holds still. (The old 1s dot cycle pulsed at a different cadence
 * than the glyph, which reads as jitter, not liveliness.) Carries NO elapsed
 * time or token count — those live in the footer's `active`/`ctx` segments.
 */
export function buildWorkingMessage(theme?: PaintTheme): string {
  return `${paint(theme, 'brand', WORKING_WORD)}${paint(theme, 'link', '…')}`;
}

export interface FooterInput {
  /** Unknown until Pi has measured a real request (not zero). */
  tokens?: number;
  contextWindow: number;
  completedTurns: number;
  activeTurnMs?: number;
  lastTurnMs?: number;
  sessionMs: number;
  activeWorkers: number;
  /** Total spawned worker records still tracked in the session ledger. */
  workerTotal?: number;
  /** Workers waiting on the lead (normalized [BLOCKED]). */
  blockedWorkers?: number;
  /** Workers that failed / crashed. */
  failedWorkers?: number;
  /** Live progress note for the most-recent running worker (name or its deltaSummary). */
  agentDoing?: string;
  /** Awareness agents present in this workspace, shown in the lower toolbar. */
  awarenessPeers?: number;
  /** Pre-session working-tree files not yet touched this session (likely peer/user WIP). */
  peerDirty?: number;
  /** Unread Awareness messages addressed to this session's agent. */
  awarenessUnread?: number;
  /** Active model-dial label (e.g. the dial preset name), shown as a branded segment. */
  dial?: string;
  /**
   * Session permission level from the approval gate. ALWAYS rendered when
   * provided — the gate's mode is safety context for every command.
   */
  permissionLevel?: string;
  /** Count of action classes the user "always allowed" this session. */
  approvedClassCount?: number;
  /** GitHub credential health resolved through `npx octocode auth status --json`. */
  githubAuth?: 'checking' | 'authenticated' | 'missing' | 'error';
  /**
   * Initial provider subtotal (frozen system prompt + direct tool contracts),
   * for the context-breakdown segment. Estimated tokens use the ~4 chars/token
   * heuristic. Distinct from the live `ctx` running-total gauge.
   */
  overhead?: { totalChars: number; sysChars: number; mcpServers: number; mcpTools: number; skills: number };
  branch?: string;
  dirty: boolean;
  /** Changed-file count from the same porcelain probe that sets `dirty`. */
  dirtyFiles?: number;
}

/** A footer segment plus the semantic colour it should paint with (default: dim). */
export interface FooterSegment {
  text: string;
  token?: SemanticToken;
  /**
   * Static bold emphasis — reserved for act-on-me states (blocked/failed
   * workers, unread peer mail, near-full context). Never animated: emphasis
   * must MEAN "act on me", and a moving footer is noise.
   */
  attention?: boolean;
}

function ellipsize(text: string, max: number): string {
  const clean = text.replace(/\s+/g, ' ').trim();
  // Cell-width aware: CJK/emoji agent labels are 2 cells each, so a byte-length
  // cap would let the footer segment overflow its budget.
  return truncateToWidth(clean, max);
}

/**
 * Footer density modes (review follow-up: the toolbar should be tunable noise).
 * - compact: high-signal only — context gauge, worker count, blocked/failed
 *   attention flags, git branch.
 * - default: everything except the session-duration segment (rarely actionable).
 * - full: every segment, including session duration.
 */
export type FooterDensity = 'compact' | 'default' | 'full';

let footerDensity: FooterDensity = 'default';

export function getFooterDensity(): FooterDensity {
  return footerDensity;
}

export function setFooterDensity(density: FooterDensity): void {
  footerDensity = density;
}

/** Parse a user-supplied density name; undefined for anything unrecognized. */
export function parseFooterDensity(value: string | undefined): FooterDensity | undefined {
  const normalized = (value ?? '').trim().toLowerCase();
  return normalized === 'compact' || normalized === 'default' || normalized === 'full' ? normalized : undefined;
}

/**
 * Ordered footer segments with per-segment colour. Optional segments (agents,
 * attention flags, git) are dropped when empty. Attention flags
 * (blocked/failed workers) get warning/error colour so a stuck worker is
 * visible in the toolbar without opening /octocode-agents. `density` trims
 * lower-signal segments; it defaults to the session-wide mode set by
 * /octocode-footer.
 */
export function buildFooterSegments(input: FooterInput, density: FooterDensity = footerDensity): FooterSegment[] {
  const segs: FooterSegment[] = [];
  const compact = density === 'compact';

  if (input.contextWindow > 0 && input.tokens !== undefined && Number.isFinite(input.tokens)) {
    const gauge = contextGauge((input.tokens / input.contextWindow) * 100);
    const exact = density === 'full'
      ? `${SEP}${formatCompact(input.tokens)}/${formatCompact(input.contextWindow)}`
      : '';
    segs.push({
      text: `context ${gauge.bar} ${gauge.pct}%${exact}`,
      token: gauge.token,
      // Near-full context is emphasized — the one gauge state that demands action.
      attention: gauge.token === 'error',
    });
  }

  if (!compact) {
    // ONE merged timing segment — a bare `active 14s` read as ambiguous
    // (turn time? session time?). Live: `turn 8 · 14s` (current turn number +
    // its elapsed). Idle: `turns 7 · last 12s`. Placeholders never render:
    // before the first turn there is nothing to count or time.
    if (input.activeTurnMs !== undefined) {
      segs.push({ text: `turn ${input.completedTurns + 1}${SEP}${formatDurationShort(input.activeTurnMs)}` });
    } else {
      const idleParts = [
        ...(input.completedTurns > 0 ? [`turns ${input.completedTurns}`] : []),
        ...(input.lastTurnMs !== undefined ? [`last ${formatDurationShort(input.lastTurnMs)}`] : []),
      ];
      if (idleParts.length > 0) segs.push({ text: idleParts.join(SEP) });
    }

    // Session uptime at default density — the one clock users actually look
    // for (previously buried in `full`).
    segs.push({ text: `session ${formatDurationShort(input.sessionMs)}` });
  }

  const workerTotal = input.workerTotal ?? input.activeWorkers;
  if (workerTotal > 0) {
    const active = input.activeWorkers > 0 ? ` (${input.activeWorkers} live)` : '';
    segs.push({ text: `agents ${workerTotal}${active}` });
  }
  // Unread peer messages are an attention flag (shown even in compact): a
  // co-working agent is waiting on a reply.
  if (input.awarenessUnread && input.awarenessUnread > 0) {
    segs.push({ text: `mail ${input.awarenessUnread}`, token: 'link', attention: true });
  }
  if (input.blockedWorkers && input.blockedWorkers > 0) {
    segs.push({ text: `blocked ${input.blockedWorkers}`, token: 'warning', attention: true });
  }
  if (input.failedWorkers && input.failedWorkers > 0) {
    segs.push({ text: `failed ${input.failedWorkers}`, token: 'error', attention: true });
  }

  if (!compact && input.dial) {
    segs.push({ text: `dial ${input.dial}`, token: 'brand' });
  }

  // Consent state is ALWAYS visible (every density): the gate's mode governs
  // every command the agent runs, so the operator should never have to wonder
  // which mode is live. `+N` = session-wide "always allow" grants. Plain-text
  // label (no shield glyph — ⛨ is ambiguous-width and would drift the row).
  // Color = risk: relaxed is the one warning; strict/default stay calm dim.
  if (input.permissionLevel) {
    const grants = input.approvedClassCount && input.approvedClassCount > 0 ? ` +${input.approvedClassCount}` : '';
    segs.push({
      text: `perm ${input.permissionLevel}${grants}`,
      token: input.permissionLevel === 'relaxed' ? 'warning' : 'dim',
    });
  }

  // Credential health is always visible at every density. Missing/error states
  // are act-on-me conditions; `/commands` carries the login instructions.
  if (input.githubAuth === 'authenticated') {
    segs.push({ text: 'github ✓', token: 'success' });
  } else if (input.githubAuth === 'missing') {
    segs.push({ text: 'github ✗ login required', token: 'error', attention: true });
  } else if (input.githubAuth === 'error') {
    segs.push({ text: 'github check failed', token: 'error', attention: true });
  } else if (input.githubAuth === 'checking') {
    segs.push({ text: 'github checking…', token: 'dim' });
  }

  // Initial provider subtotal: frozen prompt plus direct tool contracts, always
  // shown (even in compact). Default/full densities also expose
  // the live capability counts separately so users can see MCP connectivity and
  // skill discovery without decoding the prompt budget segment.
  if (input.overhead && input.overhead.totalChars > 0) {
    const o = input.overhead;
    const tok = (chars: number): string => formatCompact(estimateTokens(chars));
    const breakdown = density === 'full'
      ? ` (sys ${tok(o.sysChars)} · mcp ${o.mcpServers}/${o.mcpTools} · skills ${o.skills})`
      : '';
    segs.push({ text: `initial ~${tok(o.totalChars)}${breakdown}`, token: 'dim' });
    if (!compact && density !== 'full') {
      // One merged segment instead of two separate ones — /octocode-harness
      // shows the full breakdown with sources, tool names, and descriptions.
      // Full density already carries these counts inside the prompt breakdown.
      segs.push({ text: `mcp ${o.mcpServers}${SEP}skills ${o.skills}`, token: 'dim' });
    }
  }

  if (input.branch) {
    segs.push({ text: formatBranchSegment(input.branch, input.dirty, input.dirtyFiles) });
  }

  return segs;
}

/** `main` · `main (dirty)` · `main (5 changed)` — words instead of `*` / `Δ`. */
export function formatBranchSegment(branch: string, dirty: boolean, dirtyFiles?: number): string {
  if (!dirty) return branch;
  return dirtyFiles ? `${branch} (${dirtyFiles} changed)` : `${branch} (dirty)`;
}

// ─── Per-subagent footer rows ──────────────────────────────────────────────────

/** The minimal ledger shape the footer needs (subset of WorkerLedgerEntry). */
export interface AgentFooterEntry {
  agentId: string;
  name: string;
  status: string;
  model?: string;
  task?: string;
  planStep?: string;
  /** Structured worker result status; overrides an idle RPC process when the turn is done/blocked/failed. */
  normalizedStatus?: string;
  startedAt: string;
  updatedAt: string;
  deltaSummary?: string;
  pendingMessages?: number;
  lastMessage?: WorkerMessageActivity;
  activeTool?: string;
  toolCallCount?: number;
  toolNames?: string[];
}

export interface AgentFooterRow {
  /** Leading label, e.g. `agent researcher (a1b2)`. */
  label: string;
  /** State word (`running`, `blocked`, …). */
  state: string;
  /** Colour for the state word. */
  token: SemanticToken;
  /** Bold state word — only for act-on-me states. */
  attention: boolean;
  /** `14s` / `1m 3s` — live elapsed for active workers, total for finished ones. */
  elapsed: string;
  /** Effective model selected for this worker. */
  model?: string;
  /** Stable assignment; unlike `doing`, this does not change with tool events. */
  task?: string;
  /** Parent-plan step associated with the assignment, when supplied. */
  planStep?: string;
  /** What the worker is doing right now (ellipsized), if known. */
  doing?: string;
}

const AGENT_TERMINAL = new Set(['done', 'failed', 'killed', 'completed', 'exited', 'error']);

function effectiveAgentStatus(entry: AgentFooterEntry): string {
  const processStatus = entry.status.toLowerCase();
  if (AGENT_TERMINAL.has(processStatus)) return processStatus;
  const normalized = entry.normalizedStatus?.toLowerCase();
  if (normalized === 'done' || normalized === 'failed' || normalized === 'blocked') return normalized;
  if ((entry.pendingMessages ?? 0) > 0 && processStatus === 'idle') return 'queued';
  return processStatus;
}

const AGENT_DOING_MAX = 96;

function agentStateToken(status: string): { token: SemanticToken; attention: boolean } {
  switch (status) {
    case 'failed':
    case 'error': return { token: 'error', attention: true };
    case 'blocked': return { token: 'warning', attention: true };
    case 'killed': return { token: 'warning', attention: false };
    case 'done':
    case 'completed':
    case 'exited': return { token: 'success', attention: false };
    case 'running': return { token: 'brand', attention: false };
    case 'queued': return { token: 'link', attention: false };
    default: return { token: 'dim', attention: false };
  }
}

/**
 * One footer row per subagent — live workers first (most recently updated
 * first), then finished ones — so the operator sees every worker's name,
 * state, and current activity without opening /octocode-agents. Pure: pass
 * `nowMs` for deterministic elapsed times. Returns at most
 * AGENT_FOOTER_MAX_ROWS rows plus an `overflow` count.
 */
export function buildAgentFooterRows(
  entries: readonly AgentFooterEntry[],
  nowMs: number = Date.now(),
): { rows: AgentFooterRow[]; overflow: number } {
  const isLive = (e: AgentFooterEntry): boolean => !AGENT_TERMINAL.has(effectiveAgentStatus(e));
  const ordered = [...entries].sort((a, b) => {
    const liveDelta = Number(isLive(b)) - Number(isLive(a));
    return liveDelta !== 0 ? liveDelta : Date.parse(b.updatedAt) - Date.parse(a.updatedAt);
  });
  // No row cap — show every agent so the operator sees all workers without
  // opening /octocode-agents.
  const rows = ordered.map((e): AgentFooterRow => {
    const status = effectiveAgentStatus(e);
    const { token, attention } = agentStateToken(status);
    const started = Date.parse(e.startedAt);
    const ended = isLive(e) ? nowMs : Date.parse(e.updatedAt);
    const elapsedMs = Number.isFinite(started) && Number.isFinite(ended) ? Math.max(0, ended - started) : undefined;
    const live = isLive(e);
    const pending = e.pendingMessages ?? 0;
    const messageActivity = e.lastMessage
      ? `msg${e.lastMessage.direction === 'to-agent' ? '→' : '←'} ${e.lastMessage.action}${pending > 0 ? ` (${pending} queued)` : ''}: ${e.lastMessage.preview}`
      : pending > 0
        ? `msg→ ${pending} queued`
        : undefined;
    const activityParts = [
      ...(messageActivity ? [messageActivity] : []),
      ...(live && e.activeTool ? [`tool ${e.activeTool}`] : []),
      ...(live && e.deltaSummary ? [e.deltaSummary] : []),
      ...(!e.activeTool && e.toolCallCount && e.toolCallCount > 0
        ? [`${e.toolCallCount} call${e.toolCallCount === 1 ? '' : 's'}${e.toolNames?.length ? ` [${e.toolNames.join(',')}]` : ''}`]
        : []),
    ];
    const doing = activityParts.length > 0 ? ellipsize(activityParts.join(' · '), AGENT_DOING_MAX) : undefined;
    return {
      label: `agent ${e.name} (${e.agentId.slice(0, 6)})`,
      state: status,
      token,
      attention,
      elapsed: formatDurationShort(elapsedMs),
      model: e.model,
      task: e.task ? ellipsize(e.task.replace(/\s+/g, ' '), AGENT_DOING_MAX) : undefined,
      planStep: e.planStep ? ellipsize(e.planStep.replace(/\s+/g, ' '), AGENT_DOING_MAX) : undefined,
      doing,
    };
  });
  return { rows, overflow: 0 };
}

export interface ShortcutHint {
  /** Resolved key text, e.g. "shift+tab". Blank keys are dropped (host bound nothing). */
  key: string;
  /** Short action label, e.g. "think". */
  label: string;
  /** Semantic colour for the action label. */
  token?: SemanticToken;
  /** Optional semantic colour for this specific keycap. */
  keyToken?: SemanticToken;
}

/**
 * Build the compact keyboard-shortcut hint row rendered under the footer
 * metrics, so the highest-value shortcuts (cycle thinking, cycle permission
 * level, model select, expand tools, command palette, stop) are discoverable
 * without opening the docs. Entries whose key is blank are dropped (the host
 * didn't bind that action); the row is empty when nothing resolves. Rendered as
 * bold+dim keycaps (uniform color tier) and semantically-coloured action labels
 * (think=link, perm=warning, model=brand, tools=symbol, stop=error) when a
 * theme is available, falling back to plain `key label · key label` in
 * tests/plain mode.
 */
export function buildShortcutHintsRow(hints: readonly ShortcutHint[], theme?: PaintTheme): string {
  const fallbackKeyTokens: readonly SemanticToken[] = ['brand', 'link', 'brandAlt', 'path', 'symbol', 'error'];
  return hints
    .filter((h) => !!h.key?.trim() && !!h.label?.trim())
    .map((h, index) => {
      const keyToken = h.keyToken ?? fallbackKeyTokens[index % fallbackKeyTokens.length] ?? 'bright';
      const keyText = (h.key ?? '').trim();
      const labelText = (h.label ?? '').trim();
      const key = theme ? theme.bold(paint(theme, keyToken, keyText)) : keyText;
      const label = paint(theme, h.token ?? 'muted', labelText);
      return `${key} ${label}`;
    })
    .join(SEP);
}

/**
 * A slash-command entry for the footer discovery row.
 * `token` colors the command name; `desc` is a one-word label painted dim.
 */
export interface CommandEntry {
  /** Bare command name without the leading '/'. */
  name: string;
  /** One-word description rendered dim next to the name. */
  desc: string;
  /** Semantic color applied to the command name (meaningful per category). */
  token: SemanticToken;
}

/**
 * Build the slash-command discovery row shown below the shortcut-hints row.
 * Each entry is rendered as:
 *   dim "/" + colored name + dim desc
 * matching the two-tone keys-row pattern: chrome/key in one tier, action
 * label in a second tier. Colors are LCI-aligned — spread across the
 * perceptual hue circle — so each command category reads at a glance.
 * The list is truncated at the terminal width by the caller.
 */
export function buildCommandsRow(commands: readonly CommandEntry[], theme?: PaintTheme): string {
  if (commands.length === 0) return '';
  return commands
    .map(({ name, desc, token }) =>
      `${paint(theme, 'dim', '/')}${paint(theme, token, name)} ${paint(theme, 'dim', desc)}`)
    .join(SEP);
}


/** Map macOS `AppleInterfaceStyle` ("Dark" when dark; unset otherwise) to our theme names. */
export function resolveSystemTheme(appleInterfaceStyle: string | null | undefined): OctocodeThemeName {
  return String(appleInterfaceStyle ?? '').trim().toLowerCase() === 'dark' ? OCTOCODE_THEME_DARK : OCTOCODE_THEME_LIGHT;
}

export interface SystemThemeSignals {
  platform: NodeJS.Platform | string;
  /** macOS `defaults read -g AppleInterfaceStyle` output ("Dark" only in dark mode). */
  appleInterfaceStyle?: string;
  /** Terminal COLORFGBG env, "fg;bg" (bg 0-6 = dark, 7/15 = light). */
  colorfgbg?: string;
}

/**
 * Cross-platform system theme detection. macOS is always decidable from
 * AppleInterfaceStyle; other platforms use the terminal's COLORFGBG background
 * code. Returns null when nothing decisive is available so callers can KEEP the
 * current theme instead of wrongly forcing light.
 */
export function resolveSystemThemeName(signals: SystemThemeSignals): OctocodeThemeName | null {
  if (signals.platform === 'darwin') {
    return resolveSystemTheme(signals.appleInterfaceStyle);
  }
  const cfb = String(signals.colorfgbg ?? '').trim();
  if (!cfb) return null;
  const parts = cfb.split(';');
  const bgRaw = parts[parts.length - 1];
  const bg = Number(bgRaw);
  if (!Number.isInteger(bg) || bg < 0 || bg > 15) return null;
  // Standard terminal palette: 0-6 (+8-14) are dark backgrounds; 7 and 15 are light.
  return bg === 7 || bg === 15 ? OCTOCODE_THEME_LIGHT : OCTOCODE_THEME_DARK;
}

const SESSION_NAME_MAX = 48;

/** First non-empty line, whitespace-collapsed, truncated to a session-name length. */
export function deriveSessionName(text: string): string {
  const firstLine = String(text ?? '').split('\n').map((l) => l.trim()).find(Boolean) ?? '';
  const clean = firstLine.replace(/\s+/g, ' ').trim();
  // Cell-width aware: CJK/emoji names are 2 cells each and must not overflow or be
  // sliced mid-surrogate the way a code-unit .slice would.
  return truncatePlainToWidth(clean, SESSION_NAME_MAX);
}

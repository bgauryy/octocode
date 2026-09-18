/**
 * ui-extras — pure formatting helpers for Octocode's TUI surfaces.
 *
 * Kept side-effect-free so they can be unit-tested and reused by the footer,
 * working indicator, theme sync, and session-naming wiring in index.ts.
 */

import {
  paint,
  SEP,
  type PaintTheme,
  type SemanticToken,
} from './tui/palette.js';
// Route width helpers through render-helpers (which sanitizes tabs/control chars) rather
// than raw pi-tui, so footer/session strings are measured and cut at the true cell width.
import { truncatePlainToWidth } from './tools/render-helpers.js';
import { estimateTokens } from './utils.js';
import type { WorkerMessageActivity } from './types.js';

export const OCTOCODE_SPINNER_FRAMES = [
  '✦',
  '✧',
  '✶',
  '✺',
  '✹',
  '✷',
  '✶',
  '✧',
] as const;
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
export function buildWorkingIndicator(
  theme?: PaintTheme
): WorkingIndicatorConfig {
  return {
    frames: OCTOCODE_SPINNER_FRAMES.map((frame, index) =>
      paint(
        theme,
        OCTOCODE_SPINNER_TOKENS[index % OCTOCODE_SPINNER_TOKENS.length] ??
          'brand',
        frame
      )
    ),
    intervalMs: OCTOCODE_SPINNER_INTERVAL_MS,
  };
}

/** Shipped theme ids (single source of truth — used by the theme command + sync). */
export const OCTOCODE_THEME_DARK = 'octocode-dark';
export const OCTOCODE_THEME_LIGHT = 'octocode-light';
export type OctocodeThemeName =
  typeof OCTOCODE_THEME_DARK | typeof OCTOCODE_THEME_LIGHT;

/** 1234 → "1.2k", 45_000_000 → "45M", <1000 → as-is. */
export function formatCompact(value: number): string {
  if (!Number.isFinite(value) || value < 1000)
    return String(Math.max(0, Math.round(value || 0)));
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

/** Zero/missing timestamps mean not observed, never the Unix epoch. */
export function elapsedSince(startedAt: number | undefined, now: number): number | undefined {
  return startedAt !== undefined && Number.isFinite(startedAt) && startedAt > 0
    ? Math.max(0, now - startedAt)
    : undefined;
}

export interface CapabilityMetrics {
  dial?: string;
  /** Estimated provider subtotal, separate from measured context occupancy. */
  overhead?: {
    totalChars: number;
    sysChars: number;
    mcpServers: number;
    mcpTools: number;
    skills: number;
  };
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

/** Compact, automatic, and expanded budgets are selected by the status policy. */
export type FooterDensity = 'compact' | 'default' | 'full';

let footerDensity: FooterDensity = 'default';

export function getFooterDensity(): FooterDensity {
  return footerDensity;
}

export function setFooterDensity(density: FooterDensity): void {
  footerDensity = density;
}

/** Parse a user-supplied density name; undefined for anything unrecognized. */
export function parseFooterDensity(
  value: string | undefined
): FooterDensity | undefined {
  const normalized = (value ?? '').trim().toLowerCase();
  return normalized === 'compact' ||
    normalized === 'default' ||
    normalized === 'full'
    ? normalized
    : undefined;
}

/** Optional capability diagnostics; session, context, and worker state have their own rows. */
export function buildCapabilitySegments(
  input: CapabilityMetrics,
  density: FooterDensity = footerDensity
): FooterSegment[] {
  if (density === 'compact') return [];
  const segments: FooterSegment[] = [];
  if (input.dial) segments.push({ text: `dial ${input.dial}`, token: 'brand' });
  if (input.overhead && input.overhead.totalChars > 0) {
    const overhead = input.overhead;
    const tokens = (chars: number): string =>
      formatCompact(estimateTokens(chars));
    const breakdown =
      density === 'full'
        ? ` (sys ${tokens(overhead.sysChars)} · mcp ${overhead.mcpServers}/${overhead.mcpTools} · skills ${overhead.skills})`
        : '';
    segments.push({
      text: `initial ~${tokens(overhead.totalChars)}${breakdown}`,
      token: 'dim',
    });
    if (density !== 'full')
      segments.push({
        text: `mcp ${overhead.mcpServers}${SEP}skills ${overhead.skills}`,
        token: 'dim',
      });
  }
  return segments;
}

// ─── Worker state projection ───────────────────────────────────────────────────

/** The minimal ledger shape the footer needs (subset of WorkerLedgerEntry). */
export interface AgentFooterEntry {
  agentId: string;
  name: string;
  status: string;
  model?: string;
  task?: string;
  planStep?: string;
  cohortId?: string;
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

/** Map macOS `AppleInterfaceStyle` ("Dark" when dark; unset otherwise) to our theme names. */
export function resolveSystemTheme(
  appleInterfaceStyle: string | null | undefined
): OctocodeThemeName {
  return String(appleInterfaceStyle ?? '')
    .trim()
    .toLowerCase() === 'dark'
    ? OCTOCODE_THEME_DARK
    : OCTOCODE_THEME_LIGHT;
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
export function resolveSystemThemeName(
  signals: SystemThemeSignals
): OctocodeThemeName | null {
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
  const firstLine =
    String(text ?? '')
      .split('\n')
      .map(l => l.trim())
      .find(Boolean) ?? '';
  const clean = firstLine.replace(/\s+/g, ' ').trim();
  // Cell-width aware: CJK/emoji names are 2 cells each and must not overflow or be
  // sliced mid-surrogate the way a code-unit .slice would.
  return truncatePlainToWidth(clean, SESSION_NAME_MAX);
}

import { truncateToWidth, visibleWidth } from '../tui/width.js';
/**
 * Pure functions for rendering the Octocode banner and tagline.
 *
 * All output is width-safe: every line is measured and truncated through the
 * same ANSI-aware helpers used by the rest of the extension so pi's TUI never
 * sees a line whose visible width exceeds the terminal width.
 */

import { truncatePlainToWidth } from '../tools/render-helpers.js';
import { BETA_ISSUES_PREFIX, BETA_ISSUES_URL, BETA_LABEL, TAGLINE } from '../tui/content.js';
import { paint, SEP, type SemanticToken } from '../tui/palette.js';

// ─── Minimal theme interface ──────────────────────────────────────────────────

/** Subset of PiTheme required by banner functions. */
export interface BannerTheme {
  fg(color: string, text: string): string;
  bold(text: string): string;
}

// ─── Constants ────────────────────────────────────────────────────────────────


/**
 * Octocode banner art: the block-style OCTOCODE CODE wordmark
 * (figlet "ANSI Shadow" face) painted by renderWordmarkLines with a
 * vibrant purple→teal→purple gradient.
 */
const WORDMARK_ART: readonly string[] = [
  ' ██████╗  ██████╗████████╗ ██████╗  ██████╗ ██████╗ ██████╗ ███████╗   ██████╗ ██████╗ ██████╗ ███████╗',
  '██╔═══██╗██╔════╝╚══██╔══╝██╔═══██╗██╔════╝██╔═══██╗██╔══██╗██╔════╝  ██╔════╝██╔═══██╗██╔══██╗██╔════╝',
  '██║   ██║██║        ██║   ██║   ██║██║     ██║   ██║██║  ██║█████╗    ██║     ██║   ██║██║  ██║█████╗',
  '██║   ██║██║        ██║   ██║   ██║██║     ██║   ██║██║  ██║██╔══╝    ██║     ██║   ██║██║  ██║██╔══╝',
  '╚██████╔╝╚██████╗   ██║   ╚██████╔╝╚██████╗╚██████╔╝██████╔╝███████╗  ╚██████╗╚██████╔╝██████╔╝███████╗',
  ' ╚═════╝  ╚═════╝   ╚═╝    ╚═════╝  ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝   ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝',
];

const WORDMARK_WIDTH = WORDMARK_ART.reduce((max, line) => Math.max(max, visibleWidth(line)), 0);

/** Emoji lens+octopus mark prefixing the compact brand line (same glyphs as the HTML page / octocode CLI). */
const BRAND_MARK_EMOJI = '🔍🐙';

/** Product name shown after the emoji mark on the compact brand line. */
const BRAND_NAME = 'Octocode';

/** Static purple-family gradient used when the full wordmark cannot fit. */
const COMPACT_BRAND_RAMP: readonly SemanticToken[] = [
  'link',
  'brand',
  'title',
  'muted',
  'brand',
  'link',
  'title',
  'muted',
];

// ─── True-colour wave gradient ────────────────────────────────────────────────
//
// Deliberately NOT animated: the banner is a transcript entry at the top of
// the scrollback, and any time-varying bytes there invalidate pi-tui's line
// diff for everything below it on every repaint — which surfaced as scroll
// jumps while the model streamed.
//
// Formula (static snapshot, no time variable):
//   wave = 0.28·A + 0.22·B + 0.20·C + 0.18·D + 0.12·E + 0.08·micro
//   hue  = 278 + wave·42    →  250 … 320  (blue-violet → magenta)
//   sat  = 88  + wave·9
//   lig  = 56  + wave·20
//
// Every character gets its own value via `micro = sin(col·17.391 + row·31.719)`
// so no two neighbours share the exact hex — true per-pixel colour variety.
// Output uses ANSI 24-bit true-colour (ESC[38;2;R;G;Bm), which pi-tui's
// AnsiCodeTracker already parses and preserves across line-wraps.

/** HSL (degrees, %, %) → clamped [r, g, b] byte triple. */
function hslToRgb(h: number, s: number, l: number): readonly [number, number, number] {
  h = ((h % 360) + 360) % 360;
  s /= 100; l /= 100;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number): number => {
    const k = (n + h / 30) % 12;
    return Math.round(255 * Math.max(0, Math.min(1, l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1))));
  };
  return [f(0), f(8), f(4)] as const;
}

/** Wrap `ch` in an ANSI 24-bit foreground colour; reset immediately after. */
function trueColorChar(r: number, g: number, b: number, ch: string): string {
  return `\x1b[38;2;${r};${g};${b}m${ch}\x1b[0m`;
}

/**
 * Compute the wave-gradient RGB colour for a single banner character.
 *
 * @param col       Column index within the plain (pre-clip) art line.
 * @param row       Row index within WORDMARK_ART (0-based).
 * @param lineWidth Visible width of the clipped art line (for normalisation).
 */
function waveCharColor(
  col: number,
  row: number,
  lineWidth: number,
): readonly [number, number, number] {
  const nx = col / Math.max(1, lineWidth - 1);           // 0 → 1 horizontal
  const ny = row / Math.max(1, WORDMARK_ART.length - 1); // 0 → 1 vertical

  // Five overlapping sine waves produce an organic interference pattern.
  const A = Math.sin(nx * Math.PI * 5.0  + 0.20);                       // horizontal roll
  const B = Math.sin(ny * Math.PI * 3.5  + 0.80);                       // vertical roll
  const C = Math.cos((nx + ny) * Math.PI * 4.5 + 0.50);                // diagonal sweep
  const D = Math.sin(nx * Math.PI * 7.0  - ny * Math.PI * 2.5 + 0.30); // skewed wave
  const E = Math.cos(nx * Math.PI * 2.0  + ny * Math.PI * 6.0 + 1.00); // cross-wave

  // Deterministic per-character micro-noise — every glyph gets a unique hex.
  const micro = Math.sin(col * 17.391 + row * 31.719) * 0.08;

  // Weighted mix in [-1, 1]
  const wave = A * 0.28 + B * 0.22 + C * 0.20 + D * 0.18 + E * 0.12 + micro;

  // Purple spectrum: hue 250 (blue-violet) ↔ 320 (hot magenta)
  const hue = 278 + wave * 42;
  const sat = 88  + wave * 9;
  const lig = 56  + wave * 20;

  return hslToRgb(hue, sat, lig);
}

// ─── Public API ──────────────────────────────────────────────────────────────

/**
 * The banner art painted with the static brand gradient. Pure in (theme,
 * width): identical input → byte-identical output, so repaints are free.
 * Width-safe: the PLAIN art is clipped first (truncatePlainToWidth injects no
 * SGR resets), then the surviving glyphs are painted.
 */
export function renderWordmarkLines(theme: BannerTheme, width: number): string[] {
  // Narrow terminals: the art cannot survive a hard clip (each row degrades to
  // a mid-letter fragment + "…"), so below WORDMARK_WIDTH fall back to the
  // compact brand mark. Still pure in (theme, width) — no animation.
  //
  // HEIGHT STABILITY: always return exactly WORDMARK_ART.length lines, even
  // in compact mode. The banner is the FIRST entry in the transcript, so its
  // line number is 0 in the document. If its height changes (1 vs 6 lines)
  // on a terminal resize across the WORDMARK_WIDTH boundary, pi-tui's
  // differential renderer sees firstChanged=0 < viewportTop → fullRender(true)
  // → clears scrollback → user loses their scroll position. Padding with empty
  // strings keeps the height constant; the blank rows are invisible above
  // committed messages in a live session.
  if (width < WORDMARK_WIDTH) {
    const name = [...BRAND_NAME]
      .map((ch, index) => paint(theme, COMPACT_BRAND_RAMP[index] ?? 'brand', ch))
      .join('');
    const mark = `${BRAND_MARK_EMOJI} ${name}`;
    const lines: string[] = [truncateToWidth(mark, width)];
    while (lines.length < WORDMARK_ART.length) lines.push('');
    return lines;
  }
  return WORDMARK_ART.map((line, row) => {
    const clipped = truncatePlainToWidth(line, width);
    let painted = '';
    let col = 0;
    // Code-point iteration keeps any future astral-plane glyph in the art
    // from being split into lone surrogates by a code-unit index.
    for (const ch of clipped) {
      if (ch === ' ') {
        painted += ch;
      } else {
        const [r, g, b] = waveCharColor(col, row, clipped.length);
        painted += trueColorChar(r, g, b, ch);
      }
      col++;
    }
    return painted;
  });
}

/**
 * Build the main Octocode banner block: the colored OCTOCODE wordmark topped
 * off with the official `🔍🐙 Octocode` brand line (same mark as the published
 * `octocode` CLI), which also carries the optional version.
 *
 * Returns an array of width-safe strings (ANSI codes included) ready to be
 * passed to a pi TUI renderer. Each string is individually truncated to
 * `width` so callers can append them directly to component output.
 *
 * @param theme  A BannerTheme (fg + bold).
 * @param width  Available terminal width in columns.
 * @param version  Optional semver string shown after the wordmark, e.g. `"1.2.3"`.
 */
export function renderBannerLines(theme: BannerTheme, width: number, version?: string): string[] {
  const versionStr = version ? paint(theme, 'muted', `v${version}`) : '';
  const wordmark = renderWordmarkLines(theme, width);

  return versionStr ? [...wordmark, truncateToWidth(versionStr, width)] : wordmark;
}

/**
 * Build a single tagline line.
 *
 * @param theme  A BannerTheme (fg + bold).
 * @param width  Available terminal width in columns.
 */
export function renderTagline(theme: BannerTheme, width: number): string {
  const line = paint(theme, 'muted', TAGLINE);
  return truncateToWidth(line, width);
}

/**
 * Beta notice: gold label (this IS an act-on-me state — expect rough edges)
 * followed by a visible issue-tracker URL. Keep the URL literal instead of OSC 8
 * here: startup lines are width-sanitized/truncated, and raw URLs are more
 * reliable across terminals while still auto-linking in most emulators.
 */
export function renderBetaNotice(theme: BannerTheme, width: number): string {
  const line = `${paint(theme, 'warning', BETA_LABEL)} ${paint(theme, 'muted', `· ${BETA_ISSUES_PREFIX}`)} ${paint(theme, 'link', BETA_ISSUES_URL)}`;
  return truncateToWidth(line, width);
}

/**
 * Optional live session snapshot surfaced below the beta notice.
 * Data is captured once when the banner entry is appended (at session_start)
 * so it reads as a startup summary, not a live readout — avoids time-varying
 * bytes in a transcript entry (which would invalidate pi-tui's line diff and
 * cause scroll jumps during streaming).
 */
export interface BannerSessionInfo {
  /** Model identifier (e.g. "claude-opus-4-5"). */
  model?: string;
  /** Provider name (e.g. "anthropic"). */
  provider?: string;
  /** Thinking level active at session start (e.g. "medium"). */
  thinking?: string;
}

/**
 * Render a single muted session-info line: `model: provider/id · thinking: level`.
 * Returns `null` when there is nothing worth showing.
 */
export function renderSessionInfoLine(theme: BannerTheme, width: number, info: BannerSessionInfo): string | null {
  const parts: string[] = [];
  if (info.model && info.provider) parts.push(`model: ${info.provider}/${info.model}`);
  else if (info.model) parts.push(`model: ${info.model}`);
  if (info.thinking) parts.push(`thinking: ${info.thinking}`);
  if (parts.length === 0) return null;
  return truncateToWidth(paint(theme, 'muted', parts.join(SEP)), width);
}

/**
 * Convenience: banner lines, then the tagline, then the beta notice,
 * and optionally a session-info snapshot line when `info` is provided.
 */
export function renderBannerWithTagline(theme: BannerTheme, width: number, version?: string, info?: BannerSessionInfo): string[] {
  const lines: string[] = [...renderBannerLines(theme, width, version), renderTagline(theme, width), renderBetaNotice(theme, width)];
  if (info) {
    const infoLine = renderSessionInfoLine(theme, width, info);
    if (infoLine !== null) lines.push(infoLine);
  }
  return lines;
}

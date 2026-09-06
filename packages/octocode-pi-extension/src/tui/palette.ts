/**
 * palette — single source of truth for Octocode TUI presentation.
 *
 * The shipped `themes/octocode-dark.json` / `octocode-light.json` own the actual
 * hex values; pi resolves a *token name* passed to `theme.fg(token, text)`.
 * This module names those tokens semantically (path / link / count / …) so tool
 * renderers stop sprinkling magic strings and every surface presents the same
 * kind of data in the same colour. It also owns the terminal "hacks" that pi's
 * theme cannot express: OSC 8 hyperlinks, a NO_COLOR/FORCE_COLOR gate, and the
 * context-window gauge bar.
 */

/** C0/C1 control chars except tab (expanded below) and ESC (0x1B, ANSI). */
const CONTROL_CHAR_RE = /[\x00-\x08\x0A-\x1A\x1C-\x1F\x7F-\x9F]/g;

export const ANSI_RESET = '\u001b[0m';

/**
 * Replace tabs with 3 spaces and other control characters with a space so a string
 * renders exactly as measured: pi-tui *counts* a tab as 3 columns but emits it raw
 * (terminals advance to their own tab stops), and counts other control chars as 0
 * columns even though e.g. `\r` moves the cursor. Shared by render-helpers and
 * cli-design (which cannot import render-helpers without a cycle).
 */
export function sanitizeLine(str: string): string {
  if (!str.includes('\t') && !CONTROL_CHAR_RE.test(str)) {
    CONTROL_CHAR_RE.lastIndex = 0;
    return str;
  }
  CONTROL_CHAR_RE.lastIndex = 0;
  return str.replace(/\t/g, '   ').replace(CONTROL_CHAR_RE, ' ');
}

// ─── Semantic token names (must exist in the shipped theme `colors` map) ───────

/**
 * Semantic → theme-token mapping. Keys are how the rest of the extension should
 * think about a value ("this is a path"); values are the concrete theme tokens
 * defined in themes/octocode-*.json. Change a mapping here, not in every caller.
 */
export const TOKEN = {
  /** Brand accent (purple). */
  brand: 'accent',
  /** Secondary brand glint (teal) used as a cool counterpoint to purple. */
  brandAlt: 'syntaxOperator',
  /** File / directory paths (sky — distinct from the purple brand/title). */
  path: 'mdCode',
  /** Clickable URLs / links (lavender, matches markdown links). */
  link: 'mdLink',
  /** The raw URL tail shown after a link. */
  linkUrl: 'mdLinkUrl',
  /** Numeric counts / totals — default foreground so values pop against dim labels. */
  count: 'text',
  /** Symbol / identifier names (sky, same family as paths). */
  symbol: 'syntaxType',
  /** Tool title. */
  title: 'toolTitle',
  /** Success / ok. */
  success: 'success',
  /** Error / failure. */
  error: 'error',
  /** Warning / in-progress. */
  warning: 'warning',
  /** Secondary text. */
  muted: 'muted',
  /** Tertiary / faint text. */
  dim: 'dim',
  /** Brightest foreground — gloss / highlight moments. */
  bright: 'text',
  /** Added diff line. */
  diffAdd: 'toolDiffAdded',
  /** Removed diff line. */
  diffRemove: 'toolDiffRemoved',
  /** Unchanged diff context line. */
  diffContext: 'toolDiffContext',
} as const;

export type SemanticToken = keyof typeof TOKEN;

/** Minimal theme surface used by presentation helpers (subset of PiTheme). */
export interface PaintTheme {
  fg(color: string, text: string): string;
  bold(text: string): string;
}

/**
 * Paint `text` with a semantic token via the active theme. Falls back to the
 * raw text when no theme is available (non-TUI / plain output).
 */
export function paint(theme: PaintTheme | undefined, token: SemanticToken, text: string): string {
  return theme?.fg(TOKEN[token], text) ?? text;
}

/**
 * Paint through a Pi UI object without assuming its lazy theme getter has been
 * initialized. RPC and plain-output sessions may expose `ui` while that getter
 * still throws; status text must remain functional and unstyled in those modes.
 */
export function paintUi(
  ui: { readonly theme?: PaintTheme } | undefined,
  token: SemanticToken,
  text: string,
): string {
  try {
    return paint(ui?.theme, token, text);
  } catch {
    return text;
  }
}

// ─── Terminal capability gates ─────────────────────────────────────────────────

function envFlag(value: string | undefined): boolean {
  return value !== undefined && value !== '' && value !== '0' && value.toLowerCase() !== 'false';
}

/**
 * Whether ANSI colour should be emitted for raw (non-theme) output streams.
 *
 * Honors the https://no-color.org convention: `NO_COLOR` (any non-empty value)
 * wins and disables colour; `FORCE_COLOR` forces it on. Theme-routed rendering
 * is unaffected — pi owns that — this only guards raw escape codes we emit
 * ourselves (e.g. diff text in a tool result string).
 */
export function colorEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  // Spec-faithful no-color.org: ANY non-empty NO_COLOR disables — including
  // "0"/"false" (the launcher's gate already did this; the envFlag treatment
  // here silently ignored NO_COLOR=0, drifting from both).
  const noColor = env['NO_COLOR'];
  if (noColor !== undefined && noColor !== '') return false;
  if (envFlag(env['FORCE_COLOR'])) return true;
  if (env['TERM'] === 'dumb') return false;
  // Raw SGR codes must not leak into piped/redirected output (logs, files).
  return process.stdout?.isTTY === true;
}

/**
 * Whether to emit OSC 8 hyperlinks. Off when colour is off. Some emulators
 * mangle OSC 8 (e.g. Guacamole, certain multiplexers), so `OCTOCODE_HYPERLINKS`
 * ("0"/"false" to disable, any other value to force) provides an explicit
 * override.
 */
export function hyperlinksEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  const override = env['OCTOCODE_HYPERLINKS'];
  if (override !== undefined) return envFlag(override);
  return colorEnabled(env);
}

// ─── OSC 8 hyperlinks ───────────────────────────────────────────────────────────

const OSC = '\x1b]8;;';
const BEL = '\x07';

/**
 * Wrap `text` in an OSC 8 terminal hyperlink pointing at `url`.
 *
 * Returns `text` unchanged when hyperlinks are disabled or `url` is empty, so
 * callers can use it unconditionally. Display text defaults to the URL.
 */
export function hyperlink(url: string, text = url, env: NodeJS.ProcessEnv = process.env): string {
  if (!url || !hyperlinksEnabled(env)) return text;
  return `${OSC}${url}${BEL}${text}${OSC}${BEL}`;
}

/** True when `value` looks like an http(s) URL we can linkify. */
export function isHttpUrl(value: string): boolean {
  return /^https?:\/\/\S+$/i.test(value.trim());
}

// ─── Design constants (single source — do not inline these elsewhere) ─────────

/** Separator for dense data rows (ledger lines, tool summaries, previews). */
export const SEP = ' · ';
/** Wider separator for stacked cards and explicit detail panels. */
export const SEP_WIDE = '  ·  ';
/** The brand diamond used by headers, footer, and message cards. */
export const BRAND_DIAMOND = '◆';

// ─── Context-window gauge ───────────────────────────────────────────────────────

export interface ContextGauge {
  /** The bar glyphs, e.g. "▓▓▓░░░░░". */
  bar: string;
  /** Semantic token reflecting fill severity (success → warning → error). */
  token: SemanticToken;
  /** Number of filled cells. */
  filled: number;
  /** Clamped 0–100 percentage. */
  pct: number;
}

const GAUGE_FILL = '▓';
const GAUGE_EMPTY = '░';

/**
 * Build a fixed-width unicode gauge for context-window usage.
 *
 * Severity token: <75% success, <90% warning, ≥90% error — so a filling
 * context window visibly shifts colour before it runs out. (A green→gold→red
 * health gauge is a deliberate, conventional exception to the "green = outcomes
 * only" rule — the intent is codified in palette.test.ts.)
 *
 * @param pct    Usage percentage (clamped to 0–100).
 * @param cells  Bar width in cells (default 8, min 1).
 */
export function contextGauge(pct: number, cells = 8): ContextGauge {
  const clampedPct = Math.max(0, Math.min(100, Math.round(Number.isFinite(pct) ? pct : 0)));
  const width = Math.max(1, Math.floor(cells));
  const filled = Math.max(0, Math.min(width, Math.round((clampedPct / 100) * width)));
  const bar = GAUGE_FILL.repeat(filled) + GAUGE_EMPTY.repeat(width - filled);
  const token: SemanticToken = clampedPct >= 90 ? 'error' : clampedPct >= 75 ? 'warning' : 'success';
  return { bar, token, filled, pct: clampedPct };
}

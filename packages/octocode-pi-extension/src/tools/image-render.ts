import { truncateToWidth } from '../tui/width.js';
/**
 * image-render — inline terminal images for tool renderers (F1).
 *
 * index.ts wiring: NONE required. chrome-debug-tool.ts and browser-agent-tool.ts
 * import buildImageLines / appendImageLines directly; there is no registration
 * step and nothing to add to src/index.ts.
 *
 * Renders screenshots inline in expanded tool views via pi-tui's Image
 * component (Kitty graphics / iTerm2 inline images). Capability-gated: under
 * terminals without an image protocol (tmux, VSCode, Windows Terminal,
 * alacritty, …) a one-line themed placeholder is emitted instead.
 *
 * CRITICAL invariant: lines returned for a real image contain raw escape
 * sequences plus empty height-padding lines that pi's TUI special-cases via
 * isImageLine(). They MUST NEVER pass through truncateToWidth or any other
 * width-truncation helper — truncation would corrupt the escape payload and
 * break the TUI's image-row accounting. Everything returned by buildImageLines
 * is already final: placeholders are pre-truncated, image lines are raw.
 * Callers must append these lines verbatim (see appendImageLines).
 */

import fs from 'node:fs';
import path from 'node:path';

import { SettingsManager } from '@earendil-works/pi-coding-agent';
import { Image, detectCapabilities } from '@earendil-works/pi-tui';

import { paint } from '../tui/palette.js';
import type { PiContext, PiTheme, RenderCallReturn, RenderContext } from '../types.js';


// ─── Limits / constants ───────────────────────────────────────────────────────

/** Refuse to inline files larger than this — base64 blows up memory and the terminal. */
const MAX_IMAGE_BYTES = 4 * 1024 * 1024; // 4MB

/** ctx.state cache-key prefix so re-renders reuse the same Image instance (and Kitty image ID). */
const STATE_KEY_PREFIX = 'octocode-image:';

/** Default max rendered width in terminal cells (pi's own default, clamped to width-2 by Image). */
const MAX_WIDTH_CELLS = 60;

// ─── Mime sniffing ────────────────────────────────────────────────────────────

/**
 * Detect an image mime type from magic bytes. Supports the four formats
 * pi-tui's Image component can size-parse: png / jpeg / gif / webp.
 */
export function sniffImageMime(buf: Buffer): string | undefined {
  if (buf.length >= 8
    && buf[0] === 0x89 && buf[1] === 0x50 && buf[2] === 0x4e && buf[3] === 0x47
    && buf[4] === 0x0d && buf[5] === 0x0a && buf[6] === 0x1a && buf[7] === 0x0a) {
    return 'image/png';
  }
  if (buf.length >= 3 && buf[0] === 0xff && buf[1] === 0xd8 && buf[2] === 0xff) {
    return 'image/jpeg';
  }
  if (buf.length >= 6) {
    const head = buf.toString('latin1', 0, 6);
    if (head === 'GIF87a' || head === 'GIF89a') return 'image/gif';
  }
  if (buf.length >= 12
    && buf.toString('latin1', 0, 4) === 'RIFF'
    && buf.toString('latin1', 8, 12) === 'WEBP') {
    return 'image/webp';
  }
  return undefined;
}

// ─── Loading ──────────────────────────────────────────────────────────────────

/**
 * Load an image file for terminal rendering. Returns undefined (never throws)
 * when the file is missing, unreadable, not a regular file, larger than 4MB,
 * or not a recognized image format.
 */
export function loadImageForRender(filePath: string): { base64: string; mimeType: string } | undefined {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile() || stat.size === 0 || stat.size > MAX_IMAGE_BYTES) return undefined;
    const buf = fs.readFileSync(filePath);
    const mimeType = sniffImageMime(buf);
    if (!mimeType) return undefined;
    return { base64: buf.toString('base64'), mimeType };
  } catch {
    return undefined;
  }
}

// ─── Capability gate (injectable for tests) ───────────────────────────────────

type CapabilityCheck = () => boolean;
type ImageVisibilityCheck = (cwd: string) => boolean;

const defaultCapabilityCheck: CapabilityCheck = () => {
  try {
    // images is null under tmux/screen, VSCode, Windows Terminal, alacritty, unknown.
    return detectCapabilities().images != null;
  } catch {
    return false;
  }
};

const defaultImageVisibilityCheck: ImageVisibilityCheck = (cwd) => {
  try {
    return SettingsManager.create(cwd).getShowImages();
  } catch {
    // Pi defaults terminal.showImages to true. A settings read failure should not
    // silently disable a capability that renderContext will still enforce.
    return true;
  }
};

let capabilityCheck: CapabilityCheck = defaultCapabilityCheck;
let imageVisibilityCheck: ImageVisibilityCheck = defaultImageVisibilityCheck;

/** Test seam: override (or pass undefined to restore) the terminal-image capability check. */
export function setCapabilityCheckForTests(check?: CapabilityCheck): void {
  capabilityCheck = check ?? defaultCapabilityCheck;
}

/** Test seam for Pi's persisted terminal.showImages setting. */
export function setImageVisibilityCheckForTests(check?: ImageVisibilityCheck): void {
  imageVisibilityCheck = check ?? defaultImageVisibilityCheck;
}

/**
 * Behavioral inline-image capability used by prompt composition and image tools.
 * Pi exposes showImages only to renderers, so non-render phases read the same
 * persisted setting through its public SettingsManager API.
 */
export function effectiveInlineImages(ctx?: Pick<PiContext, 'cwd' | 'hasUI' | 'mode'>): boolean {
  if (ctx?.hasUI !== true || ctx.mode !== 'tui') return false;
  const cwd = ctx.cwd ?? process.cwd();
  return capabilityCheck() && imageVisibilityCheck(cwd);
}

/** Bounded per-turn capability projection for model routing decisions. */
export function renderRuntimeCapabilitiesAddendum(ctx?: Pick<PiContext, 'cwd' | 'hasUI' | 'mode'>): string {
  const protocolSupported = capabilityCheck();
  const effective = ctx?.hasUI === true
    && ctx.mode === 'tui'
    && protocolSupported
    && imageVisibilityCheck(ctx.cwd ?? process.cwd());
  return [
    '<runtime_capabilities>',
    `effective_inline_images: ${effective}`,
    `terminal_image_protocol_supported: ${protocolSupported}`,
    'image_browser_fallback_requires_consent: true',
    '</runtime_capabilities>',
  ].join('\n');
}

/**
 * True when the current terminal can display inline images (Kitty graphics or
 * iTerm2 protocol — Kitty/Ghostty/WezTerm/Warp/iTerm2). Respects the same test
 * seam as buildImageLines. Tools use this to decide whether to fall back to a
 * "saved to disk, offer to open" flow on terminals like VS Code/tmux/plain xterm.
 */
export function isTerminalImageCapable(): boolean {
  return capabilityCheck();
}

/**
 * The detected inline-image protocol name ("kitty" | "iterm2") or null when the
 * terminal has no image support. Used only for human-facing messages.
 */
export function terminalImageProtocol(): string | null {
  try {
    return detectCapabilities().images ?? null;
  } catch {
    return null;
  }
}

// ─── Rendering ────────────────────────────────────────────────────────────────

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function fileSize(filePath: string, base64: string): number {
  try {
    return fs.statSync(filePath).size;
  } catch {
    return Math.floor((base64.length * 3) / 4);
  }
}

/**
 * Build final, ready-to-emit lines that display `filePath` inline.
 *
 * - Unsupported terminal (or ctx.showImages === false): one placeholder line,
 *   themed and pre-truncated to `width` — safe as-is.
 * - Supported terminal: pi-tui Image lines (escape sequence + empty padding
 *   rows). These are RAW — callers must NOT run them through truncateToWidth.
 * - Unloadable / oversized / non-image file: empty array (render nothing).
 *
 * The Image instance is cached in ctx.state['octocode-image:<path>'] so
 * streaming re-renders reuse it (stable Kitty image ID, no re-encode).
 */
export function buildImageLines(
  ctx: RenderContext | undefined,
  filePath: string,
  width: number,
  theme?: PiTheme,
  opts?: { name?: string; bytes?: number },
): string[] {
  const loaded = loadImageForRender(filePath);
  if (!loaded) return [];
  const bytes = opts?.bytes ?? fileSize(filePath, loaded.base64);
  return buildImageLinesFromData(ctx, filePath, loaded.base64, loaded.mimeType, width, {
    theme,
    name: opts?.name ?? path.basename(filePath),
    bytes,
  });
}

/**
 * Data-based variant of buildImageLines: render already-in-memory base64 image
 * data (no disk read) using the same capability gate, placeholder fallback, and
 * ctx.state cache. Used by tools that already hold the bytes (`readMedia`,
 * `media`) so they don't re-read the file.
 *
 * `cacheKey` must be a stable per-slot identifier (e.g. the file path). The same
 * RAW-lines invariant applies: callers MUST NOT run the output through
 * truncateToWidth.
 */
export function buildImageLinesFromData(
  ctx: RenderContext | undefined,
  cacheKey: string,
  base64: string,
  mimeType: string,
  width: number,
  opts?: { theme?: PiTheme; name?: string; bytes?: number },
): string[] {
  if (!base64) return [];
  const theme = opts?.theme;
  const name = (opts?.name ?? path.basename(cacheKey)) || 'image';
  const supported = capabilityCheck() && ctx?.showImages !== false;

  if (!supported) {
    const bytes = opts?.bytes ?? Math.floor((base64.length * 3) / 4);
    const placeholder = paint(theme, 'dim', `🖼 image: ${name} (${formatBytes(bytes)})`);
    // Placeholder is plain themed text — truncating it here keeps the contract
    // that everything this returns is already final.
    return [truncateToWidth(placeholder, Math.max(4, width))];
  }

  const key = `${STATE_KEY_PREFIX}${cacheKey}`;
  let img = ctx?.state?.[key] as Image | undefined;
  if (!(img instanceof Image)) {
    img = new Image(
      base64,
      mimeType,
      { fallbackColor: (s: string) => paint(theme, 'dim', s) },
      { maxWidthCells: MAX_WIDTH_CELLS, filename: name },
    );
    if (ctx?.state) ctx.state[key] = img;
  }

  // Raw image lines — escape sequences + padding. NEVER truncate these.
  return img.render(width);
}

/**
 * Wrap an existing renderer so it also emits the image at `filePath` after its
 * own (width-truncated) lines. The appended image lines bypass truncation —
 * this is the only sanctioned way to combine component output with images.
 */
export function appendImageLines(
  base: RenderCallReturn,
  ctx: RenderContext | undefined,
  filePath: string,
  theme?: PiTheme,
  opts?: { name?: string; bytes?: number },
): RenderCallReturn {
  return {
    render: (width = 80) => [
      ...base.render(width),
      ...buildImageLines(ctx, filePath, width, theme, opts),
    ],
    invalidate: () => base.invalidate(),
  };
}

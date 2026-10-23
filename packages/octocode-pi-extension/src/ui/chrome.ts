import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionAPI, ExtensionContext, Theme } from '@earendil-works/pi-coding-agent';
import { mixColors, rgbColor, type Color, type TUI } from '@earendil-works/pi-tui';
import { packageRoot } from '../shared/package.js';
import { appKey, expandKey, statusColor } from '../shared/render.js';
import { stopRowTimers } from '../shared/tool-timing.js';
import { bannerLines, type BannerPaint } from './banner.js';
import { STATUS } from '../shared/status.js';
import { thinkingTranscript } from './thinking.js';
import { registerFooter } from './footer.js';

let version: { value: string | undefined } | undefined;

export function packageVersion(): string | undefined {
  if (version) return version.value;
  try {
    const manifest = path.join(packageRoot(), 'package.json');
    version = { value: (JSON.parse(fs.readFileSync(manifest, 'utf8')) as { version?: string }).version };
  } catch {
    version = { value: undefined };
  }
  return version.value;
}

/** How long the banner's light band takes to cross, and its frame interval. */
const SHINE_MS = 1_100;
const SHINE_FRAME_MS = 40;
const SPINNER = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/** Colour at `t` (0..1) along evenly spaced stops. */
function along(stops: Color[], t: number): Color {
  if (stops.length === 1) return stops[0]!;
  const scaled = Math.min(1, Math.max(0, t)) * (stops.length - 1);
  const index = Math.min(stops.length - 2, Math.floor(scaled));
  return mixColors(stops[index]!, stops[index + 1]!, scaled - index);
}

/**
 * The banner's gradient, from the theme: accent → borderAccent → mdLink → syntaxOperator (purple to teal in the
 * Octocode themes). Shadows are the same hue sunk toward `dim`; the shine lifts strokes toward white (dark themes) or
 * the text colour (light ones). Undefined when the theme has no concrete colours, so the banner uses tokens instead.
 */
export function themeRamp(theme: Theme): BannerPaint['ramp'] {
  let colors: Theme['colors'] | undefined;
  try {
    colors = theme.colors;
  } catch {
    return undefined;
  }
  if (!colors) return undefined;
  const stops = [colors.accent, colors.borderAccent, colors.mdLink, colors.syntaxOperator].filter((color): color is Color => color !== undefined);
  if (stops.length === 0 || typeof theme.style !== 'function') return undefined;
  // The dark theme's `text` is the terminal's default foreground (an empty token), which cannot be mixed: it glows white.
  const highlight = theme.appearance === 'light' ? colors.text : rgbColor(255, 255, 255);
  const cache = new Map<string, Color>();
  return (t, shade, glow, text) => {
    const key = `${t}:${shade}:${glow}`;
    let color = cache.get(key);
    if (!color) {
      color = along(stops, t);
      if (shade === 'shadow') color = mixColors(color, colors!.dim ?? color, 0.6);
      if (glow > 0 && highlight) color = mixColors(color, highlight, Math.min(1, glow * (shade === 'shadow' ? 0.35 : 0.8)));
      cache.set(key, color);
    }
    return theme.style(text, { fg: color, ...(shade === 'stroke' ? { bold: true } : {}) });
  };
}

/** The working indicator: a braille spinner whose frames walk the banner's gradient. */
export function spinnerFrames(theme: Theme): string[] {
  const ramp = themeRamp(theme);
  return SPINNER.map((frame, index) => {
    // Out and back, so the colour loops without a jump.
    const t = 1 - Math.abs(1 - (2 * index) / SPINNER.length);
    return ramp ? ramp(t, 'stroke', 0, frame) : theme.fg('accent', frame);
  });
}

/**
 * The banner as Pi's header component. `shineFrom` starts the one-time light sweep. `onInvalidate` runs when Pi
 * invalidates the header: a theme switch does, and Pi has no theme event for extensions.
 */
export function bannerHeader(tui: Pick<TUI, 'requestRender'>, theme: Theme, shineFrom: number | undefined, onInvalidate?: () => void) {
  // Read from the theme again on invalidate: Pi invalidates components when the theme changes.
  const painted = (): BannerPaint => {
    const ramp = themeRamp(theme);
    return { fg: (color, text) => theme.fg(color as never, text), ...(ramp ? { ramp } : {}) };
  };
  let paint = painted();
  const ramp = paint.ramp;
  const info = { version: packageVersion(), hints: ['/octocode help', `${expandKey()} expand`, `${appKey('app.interrupt', 'escape').replace(/^escape$/, 'esc')} interrupt`, '/agents team'] };
  let timer: ReturnType<typeof setInterval> | undefined;
  const shine = (): number | undefined => {
    if (shineFrom === undefined || !ramp) return undefined;
    const progress = (Date.now() - shineFrom) / SHINE_MS;
    if (progress >= 1) {
      stop();
      return undefined;
    }
    return Math.max(0, progress);
  };
  const stop = () => {
    if (timer) clearInterval(timer);
    timer = undefined;
    shineFrom = undefined;
  };
  if (shineFrom !== undefined && ramp) {
    timer = setInterval(() => tui.requestRender(), SHINE_FRAME_MS);
    timer.unref?.();
  }
  let cached: { width: number; shine: number | undefined; lines: string[] } | undefined;
  return {
    render(width: number): string[] {
      const now = shine();
      if (!cached || cached.width !== width || cached.shine !== now) cached = { width, shine: now, lines: bannerLines(paint, width, info, now) };
      return cached.lines;
    },
    invalidate(): void {
      cached = undefined;
      paint = painted();
      onInvalidate?.();
    },
    dispose: stop,
  };
}

/**
 * Octocode's look in the TUI: the "OCTOCODE CODE" banner as the header (in place of Pi's logo and key hints; nothing
 * is written to the session, so it stays put across /new, /resume and /fork), a two-line footer, a gradient working
 * spinner, a hidden-thinking label and the generation speed of the last run. Only time spent streaming assistant
 * messages counts toward the speed, so tool calls, subagents and askUser waits don't drag the number down.
 */
export function registerUi(pi: ExtensionAPI, isSubagent: boolean): void {
  if (isSubagent) return;
  registerFooter(pi);
  let generated = { output: 0, ms: 0 };
  let messageStartedAt: number | undefined;
  pi.on('session_start', async (event, ctx) => {
    if (!ctx.hasUI) return;
    ctx.ui.setStatus(STATUS.speed, undefined);
    // Shine once, on a fresh start with nothing below the banner. Module state cannot remember it: Pi re-imports
    // extensions for every runtime (/new, /resume, /fork, /reload). And a resumed transcript pushes the banner above
    // the viewport, where every changed frame makes pi-tui clear and redraw the whole scrollback.
    const fresh = event.reason === 'startup' && !ctx.sessionManager.getBranch().some((entry) => entry.type === 'message');
    if (ctx.mode === 'tui') decorate(ctx, fresh ? Date.now() : undefined);
  });
  pi.on('agent_start', async () => {
    generated = { output: 0, ms: 0 };
    messageStartedAt = undefined;
  });
  pi.on('message_start', async (event) => {
    if (event.message.role === 'assistant') messageStartedAt = Date.now();
  });
  pi.on('message_end', async (event) => {
    if (event.message.role !== 'assistant' || messageStartedAt === undefined) return;
    generated.ms += Date.now() - messageStartedAt;
    generated.output += event.message.usage.output;
    messageStartedAt = undefined;
  });
  pi.on('agent_end', async (_event, ctx) => {
    // Every call of the run ended. Pi drops rows it never gave a result here; their clocks must not tick on.
    stopRowTimers();
    if (!ctx.hasUI) return;
    const speed = tokensPerSecond(generated.output, generated.ms);
    if (speed !== undefined) ctx.ui.setStatus(STATUS.speed, statusColor(ctx, 'dim', `${speed.toFixed(0)} tok/s`));
  });
}

function decorate(ctx: ExtensionContext, shineFrom: number | undefined): void {
  // The spinner and thinking label are painted strings: repaint them whenever a theme switch invalidates the header.
  ctx.ui.setHeader((tui, current) => {
    const header = bannerHeader(tui, current, shineFrom, () => paintIndicators(ctx));
    const transcript = thinkingTranscript(tui);
    return {
      ...header,
      render(width: number) {
        transcript.refresh();
        return header.render(width);
      },
    };
  });
  paintIndicators(ctx);
}

function paintIndicators(ctx: ExtensionContext): void {
  const theme = ctx.ui.theme;
  // Pi's loader steps every 80 ms when no interval is given.
  ctx.ui.setWorkingIndicator({ frames: spinnerFrames(theme) });
  // Shown in place of a thinking block when `hideThinkingBlock` is on (the thinking toggle shows it again).
  ctx.ui.setHiddenThinkingLabel(`${theme.italic(theme.fg('thinkingText', '✻ Thinking…'))}${theme.fg('dim', ` (${appKey('app.thinking.toggle', 'ctrl+t')})`)}`);
}

/** Output tokens per second of streaming time, or undefined when nothing was generated. */
export function tokensPerSecond(outputTokens: number, streamingMs: number): number | undefined {
  return streamingMs > 0 && outputTokens > 0 ? outputTokens / (streamingMs / 1000) : undefined;
}

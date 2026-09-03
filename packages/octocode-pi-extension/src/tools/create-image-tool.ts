/**
 * create-image-tool — registers the `createImage` tool: turn agent-authored
 * markup into a real image and render it inline in the TUI (Kitty graphics /
 * iTerm2 inline images; a themed placeholder on terminals without image
 * support).
 *
 * Two authoring modes — the model can create ANY image, not just vector art:
 *   • svg  — rasterized with @resvg/resvg-js (Rust, zero runtime deps, prebuilt
 *            per-platform binaries). Fast, no browser. System fonts ARE loaded
 *            so text renders correctly.
 *   • html — rendered by headless Chrome (reusing the bundled CDP engine) and
 *            screenshotted to PNG. Full CSS/flex/grid/gradients/webfonts/emoji —
 *            whatever a browser can paint. Requires Chrome to be installed.
 *
 * The rendered PNG is carried in result.details (NOT result.content) so it shows
 * in the transcript without bloating model context. Pass showToModel:true to
 * also return it as an image block for a vision model to inspect. Optionally
 * saveTo persists the PNG to disk (path-guarded).
 */

import fs from 'node:fs';
import path from 'node:path';

import { Resvg } from '@resvg/resvg-js';

import type { TSchema, ToolCallResult, ToolDefinition, PiContext, PiTheme, RenderContext } from '../types.js';
import { createSessionArtifactContext } from './session-artifacts.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { cliStatusGlyph, cliStatusToken, cliToolTitle, paint } from '../tui/cli-design.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import { resolveFilePath } from './file-state.js';
import { appendImageLines, effectiveInlineImages, formatBytes, isTerminalImageCapable } from './image-render.js';
import { connectToChrome, cleanupConnection, findChromePath } from '../chrome-debug.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { extensionTmpRoot } from '../extension-paths.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

/** Refuse to render output larger than this — matches image-render's inline cap. */
const MAX_PNG_BYTES = 4 * 1024 * 1024; // 4MB
/** Guard against pathological render sizes. */
const MAX_RENDER_WIDTH = 4096;
const MAX_RENDER_HEIGHT = 8192;
/**
 * Dedicated headless port range so HTML rendering never clashes with a user's
 * chromeDebug session on 9222. Each render picks a DISTINCT port (base + a
 * rotating offset with wraparound) so two parallel createImage({html}) calls do
 * not share one headless Chrome — otherwise the first to finish SIGTERMs the
 * shared instance and kills the other mid-render. A distinct port means each
 * call launches, owns, and tears down its own instance (connectToChrome uses a
 * port-specific profile, and cleanupConnection kills only the pid it launched).
 */
const HTML_RENDER_PORT_BASE = 9445;
const HTML_RENDER_PORT_RANGE = 100; // ports 9445–9544
let htmlRenderPortCounter = 0;
let fallbackFileCounter = 0;
const implicitArtifactFiles = new Set<string>();
const implicitArtifactDirs = new Set<string>();
const fallbackRoot = (): string => path.join(extensionTmpRoot(), 'images');

/** Next distinct headless render port, rotating within the safe range. */
function nextHtmlRenderPort(): number {
  const offset = htmlRenderPortCounter % HTML_RENDER_PORT_RANGE;
  htmlRenderPortCounter = (htmlRenderPortCounter + 1) % HTML_RENDER_PORT_RANGE;
  return HTML_RENDER_PORT_BASE + offset;
}

export interface CreateImageResult {
  ok: boolean;
  message: string;
  base64?: string;
  bytes?: number;
  name?: string;
  savedPath?: string;
}

function clampInt(value: number | undefined, min: number, max: number): number | undefined {
  if (value === undefined || !Number.isFinite(value)) return undefined;
  return Math.min(Math.max(min, Math.floor(value)), max);
}

/** True for an explicitly transparent background value. */
function isTransparentBg(bg?: string): boolean {
  if (!bg) return true;
  const b = bg.trim().toLowerCase();
  return b === 'transparent' || b === 'none' || /rgba?\([^)]*,\s*0(\.0+)?\s*\)$/.test(b);
}

/**
 * Turn rendered PNG bytes into a CreateImageResult: enforce the size cap and,
 * when requested, persist to a path-guarded location. Never throws.
 */
function finalizePng(png: Buffer, cwd: string, opts: { name?: string; saveTo?: string }): CreateImageResult {
  if (png.length === 0) return { ok: false, message: 'createImage: rendered an empty image.' };
  if (png.length > MAX_PNG_BYTES) {
    return { ok: false, message: `createImage: rendered PNG is ${formatBytes(png.length)}, over the 4MB inline limit. Reduce width or complexity.` };
  }

  const name = opts.name && opts.name.trim().length > 0 ? opts.name.trim() : 'image.png';
  let savedPath: string | undefined;
  if (opts.saveTo && opts.saveTo.trim().length > 0) {
    try {
      const abs = resolveFilePath(opts.saveTo, cwd);
      assertPathAllowed(abs, cwd, 'createImage');
      fs.mkdirSync(path.dirname(abs), { recursive: true });
      fs.writeFileSync(abs, png);
      savedPath = abs;
    } catch (err) {
      return { ok: false, message: `createImage: could not save to "${opts.saveTo}" — ${(err as Error).message}` };
    }
  }

  const savedNote = savedPath ? ` → saved ${path.basename(savedPath)}` : '';
  return {
    ok: true,
    message: `Created image ${name} [image/png, ${formatBytes(png.length)}]${savedNote}`,
    base64: png.toString('base64'),
    bytes: png.length,
    name,
    savedPath,
  };
}

/**
 * Core: rasterize an SVG string to a PNG. Pure of Pi types so it is
 * unit-testable. Loads system fonts so text renders correctly. Returns ok:false
 * with a reason for invalid SVG, oversized output, or a save failure.
 */
export function createImageFromSvg(
  svg: string,
  cwd: string,
  opts: { width?: number; background?: string; name?: string; saveTo?: string } = {},
): CreateImageResult {
  if (typeof svg !== 'string' || !svg.includes('<svg')) {
    return { ok: false, message: 'createImage: `svg` must be an SVG document containing an <svg> element.' };
  }

  let png: Buffer;
  try {
    const width = clampInt(opts.width, 1, MAX_RENDER_WIDTH);
    const resvg = new Resvg(svg, {
      fitTo: width ? { mode: 'width', value: width } : { mode: 'original' },
      background: opts.background,
      // Load system fonts so <text> renders — without this, text is invisible.
      font: { loadSystemFonts: true },
      shapeRendering: 2, // geometricPrecision
      textRendering: 1, // optimizeLegibility
    });
    png = Buffer.from(resvg.render().asPng());
  } catch (err) {
    return { ok: false, message: `createImage: failed to render SVG — ${(err as Error).message}` };
  }

  return finalizePng(png, cwd, opts);
}

/** Wrap fragment HTML in a full document with sane resets + optional background. */
function wrapHtml(html: string, background?: string): string {
  const looksFull = /<html[\s>]/i.test(html) || /<body[\s>]/i.test(html) || /<!doctype/i.test(html);
  if (looksFull) return html;
  const bg = background && !isTransparentBg(background) ? `background:${background};` : '';
  return `<!doctype html><html><head><meta charset="utf-8"><style>` +
    `*{box-sizing:border-box}html,body{margin:0;padding:0;${bg}}` +
    `body{width:fit-content;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif}` +
    `</style></head><body>${html}</body></html>`;
}

/**
 * Render an HTML document to a PNG using headless Chrome via the bundled CDP
 * engine. Launches a dedicated headless instance, sizes the viewport to the
 * content (or the requested width/height), captures at 2× for crispness, and
 * always tears the browser down. Throws with a clear message if Chrome is
 * missing or rendering fails.
 */
export async function renderHtmlToPng(
  html: string,
  cwd: string,
  opts: { width?: number; height?: number; background?: string; signal?: AbortSignal } = {},
): Promise<Buffer> {
  findChromePath(); // throws a clear error if Chrome is not installed

  // Distinct port per render so concurrent HTML renders each own/launch/kill
  // their own headless Chrome instead of sharing (and killing) one another's.
  const renderPort = nextHtmlRenderPort();
  const conn = await connectToChrome({
    port: renderPort,
    launch: true,
    headless: true,
    newTab: 'about:blank',
    workspaceCwd: cwd,
    signal: opts.signal,
  });
  const { session } = conn;
  try {
    await session.send('Page.enable', {});

    const reqWidth = clampInt(opts.width, 1, MAX_RENDER_WIDTH);
    const reqHeight = clampInt(opts.height, 1, MAX_RENDER_HEIGHT);

    // Initial viewport so content reflows to the requested width before measuring.
    await session.send('Emulation.setDeviceMetricsOverride', {
      width: reqWidth ?? 800,
      height: reqHeight ?? 600,
      deviceScaleFactor: 2,
      mobile: false,
    });
    if (isTransparentBg(opts.background)) {
      await session.send('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } }).catch(() => undefined);
    }

    const frameTree = await session.send('Page.getFrameTree', {});
    const frameId = ((frameTree['frameTree'] as Record<string, unknown> | undefined)?.['frame'] as Record<string, unknown> | undefined)?.['id'] as string | undefined;
    if (!frameId) throw new Error('could not resolve the page frame');
    await session.send('Page.setDocumentContent', { frameId, html: wrapHtml(html, opts.background) });

    // Let layout settle and webfonts load.
    await new Promise((r) => setTimeout(r, 200));
    await session.send('Runtime.evaluate', {
      expression: 'document.fonts && document.fonts.ready ? document.fonts.ready.then(() => true) : true',
      awaitPromise: true,
      timeout: 3000,
    }).catch(() => undefined);

    // Measure content and re-size the viewport to fit it (unless width/height fixed).
    const metrics = await session.send('Page.getLayoutMetrics', {});
    const content = (metrics['cssContentSize'] ?? metrics['contentSize']) as { width?: number; height?: number } | undefined;
    const finalWidth = clampInt(reqWidth ?? Math.ceil(content?.width ?? 800), 1, MAX_RENDER_WIDTH)!;
    const finalHeight = clampInt(reqHeight ?? Math.ceil(content?.height ?? 600), 1, MAX_RENDER_HEIGHT)!;
    await session.send('Emulation.setDeviceMetricsOverride', {
      width: finalWidth,
      height: finalHeight,
      deviceScaleFactor: 2,
      mobile: false,
    });

    const shot = await session.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true });
    const data = shot['data'] as string | undefined;
    if (!data) throw new Error('screenshot returned no data');
    return Buffer.from(data, 'base64');
  } finally {
    // Always close the tab and kill the headless instance we launched.
    await cleanupConnection(session, false, true).catch(() => undefined);
  }
}

/**
 * Render an HTML document to a PDF using headless Chrome via the bundled CDP
 * engine (Page.printToPDF). Mirrors renderHtmlToPng — launches a dedicated
 * headless instance, injects the document, waits for fonts, and always tears the
 * browser down. Throws with a clear message if Chrome is missing or fails.
 */
export async function renderHtmlToPdf(
  html: string,
  cwd: string,
  opts: { landscape?: boolean; background?: boolean; scale?: number; signal?: AbortSignal } = {},
): Promise<Buffer> {
  findChromePath(); // throws a clear error if Chrome is not installed

  const renderPort = nextHtmlRenderPort();
  const conn = await connectToChrome({
    port: renderPort,
    launch: true,
    headless: true,
    newTab: 'about:blank',
    workspaceCwd: cwd,
    signal: opts.signal,
  });
  const { session } = conn;
  try {
    await session.send('Page.enable', {});
    const frameTree = await session.send('Page.getFrameTree', {});
    const frameId = ((frameTree['frameTree'] as Record<string, unknown> | undefined)?.['frame'] as Record<string, unknown> | undefined)?.['id'] as string | undefined;
    if (!frameId) throw new Error('could not resolve the page frame');
    await session.send('Page.setDocumentContent', { frameId, html: wrapHtml(html) });

    await new Promise((r) => setTimeout(r, 200));
    await session.send('Runtime.evaluate', {
      expression: 'document.fonts && document.fonts.ready ? document.fonts.ready.then(() => true) : true',
      awaitPromise: true,
      timeout: 3000,
    }).catch(() => undefined);

    const pdf = await session.send('Page.printToPDF', {
      printBackground: opts.background !== false,
      landscape: opts.landscape === true,
      scale: opts.scale && opts.scale > 0 ? opts.scale : 1,
      preferCSSPageSize: true,
    });
    const data = pdf['data'] as string | undefined;
    if (!data) throw new Error('printToPDF returned no data');
    return Buffer.from(data, 'base64');
  } finally {
    await cleanupConnection(session, false, true).catch(() => undefined);
  }
}

/** Async core for HTML mode: render + finalize. Never throws. */
export async function createImageFromHtml(
  html: string,
  cwd: string,
  opts: { width?: number; height?: number; background?: string; name?: string; saveTo?: string; signal?: AbortSignal } = {},
  deps: { renderHtml?: typeof renderHtmlToPng } = {},
): Promise<CreateImageResult> {
  if (typeof html !== 'string' || html.trim().length === 0) {
    return { ok: false, message: 'createImage: `html` must be non-empty HTML markup.' };
  }
  let png: Buffer;
  try {
    png = await (deps.renderHtml ?? renderHtmlToPng)(html, cwd, opts);
  } catch (err) {
    return { ok: false, message: `createImage: failed to render HTML — ${(err as Error).message}` };
  }
  return finalizePng(png, cwd, opts);
}

/**
 * Persist a rendered PNG so it can be opened when the terminal can't display
 * it inline.
 *
 * Primary: `$OCTOCODE_HOME/extension/workspaces/<workspace>/sessions/<session-key>/images/<name>.png`
 * (registered as an `image` producer in the session artifact manifest).
 * Fallback: `$OCTOCODE_HOME/extension/tmp/images/<session-id>/` when session context
 * is unavailable or the artifact dir cannot be created.
 *
 * Never throws — this is best-effort.
 */
export function persistRenderedPng(base64: string, ctx?: PiContext, name?: string): string | undefined {
  const safeBase = (name ?? 'image.png').replace(/[^\w.-]+/g, '_').replace(/\.png$/i, '') || 'image';
  fallbackFileCounter = (fallbackFileCounter + 1) % Number.MAX_SAFE_INTEGER;
  const suffix = `${Date.now()}-${fallbackFileCounter}`;

  // Primary: session artifact dir.
  if (ctx?.sessionManager) {
    try {
      const artifactCtx = createSessionArtifactContext({ cwd: ctx.cwd, sessionManager: ctx.sessionManager });
      const relPath = `images/${safeBase}-${suffix}.png`;
      const file = artifactCtx.resolve(relPath);
      fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
      fs.writeFileSync(file, Buffer.from(base64, 'base64'), { mode: 0o600 });
      artifactCtx.registerProducer('image', relPath);
      implicitArtifactFiles.add(file);
      implicitArtifactDirs.add(path.dirname(file));
      return file;
    } catch { /* fall through to OS-temp fallback */ }
  }

  // Fallback: OS temp.
  try {
    const sessionId = ctx?.sessionManager?.getSessionId?.() ?? `pid-${process.pid}`;
    const safeSession = sessionId.replace(/[^\w.-]+/g, '_').slice(0, 96) || `pid-${process.pid}`;
    const dir = path.join(fallbackRoot(), safeSession);
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    fs.chmodSync(dir, 0o700);
    const file = path.join(dir, `${safeBase}-${suffix}.png`);
    fs.writeFileSync(file, Buffer.from(base64, 'base64'), { mode: 0o600 });
    implicitArtifactFiles.add(file);
    implicitArtifactDirs.add(dir);
    return file;
  } catch {
    return undefined;
  }
}

/** Remove only harness-created fallback images; explicit saveTo output is never tracked. */
export function cleanupImplicitImageArtifacts(): number {
  let removed = 0;
  for (const file of implicitArtifactFiles) {
    try {
      fs.rmSync(file, { force: true });
      removed += 1;
    } catch {
      // Best-effort session cleanup.
    }
  }
  implicitArtifactFiles.clear();
  for (const dir of implicitArtifactDirs) {
    try { fs.rmdirSync(dir); } catch { /* non-empty or already removed */ }
  }
  implicitArtifactDirs.clear();
  try { fs.rmdirSync(fallbackRoot()); } catch { /* another session may still own it */ }
  return removed;
}

function buildParameters(Type: TypeBoxBuilder): TSchema {
  return Type.Object(
    {
      svg: Type.Optional(Type.String({ description: 'SVG document to render (must contain an <svg> element). Lightweight vector path — no browser needed. Provide svg OR html.' })),
      html: Type.Optional(Type.String({ description: 'HTML markup to render via headless Chrome — full CSS/flex/grid/gradients/webfonts/emoji. Use for rich, "any image" visuals. Provide svg OR html. Requires Chrome installed.' })),
      width: Type.Optional(Type.Integer({ minimum: 1, description: 'Target render width in pixels. SVG: scales the vector. HTML: viewport width (content reflows to it). Omit to fit content. Max 4096.' })),
      height: Type.Optional(Type.Integer({ minimum: 1, description: 'HTML only: fixed viewport height in pixels. Omit to fit content height. Max 8192.' })),
      background: Type.Optional(Type.String({ description: 'Background color (e.g. "white", "#0d1117", "rgba(0,0,0,0)"). Default: transparent.' })),
      name: Type.Optional(Type.String({ description: 'Display name for the image (shown as the placeholder label on terminals without image support).' })),
      saveTo: Type.Optional(Type.String({ description: 'Optional path to also save the rendered PNG to disk (path-guarded to the workspace).' })),
      showToModel: Type.Optional(Type.Boolean({ description: 'Also return the image to the model as a vision block. Default false (rendered in the TUI for the user only, saving context).' })),
    },
    { additionalProperties: false },
  );
}

export function registerCreateImageTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'createImage',
    label: 'Create Image',
    description:
      'Create an image from agent-authored markup and show it inline in the TUI (Kitty/iTerm2 inline images; a themed placeholder elsewhere). ' +
      'Two modes: `svg` (fast vector, rasterized with resvg, no browser) or `html` (full CSS/flex/grid/gradients/webfonts/emoji via headless Chrome — "any image"). ' +
      'Use when a diagram, chart, table, badge, or other visual communicates better than text. Optionally saves to disk and/or returns the image to a vision model (showToModel).',
    promptSnippet: 'Create an image from SVG (vector) or HTML (full CSS via headless Chrome) and show it inline in the TUI — for diagrams/charts/visuals.',
    promptGuidelines: [
      'Use createImage to SHOW something graphic (diagram/chart/table/badge). Provide `svg` for simple vector art, or `html` for rich CSS layouts, gradients, webfonts, and emoji ("any image").',
      'html mode renders in headless Chrome (needs Chrome installed); svg mode is browser-free and loads system fonts so text renders.',
      'Prefer text for textual answers. The rendered PNG stays out of model context by default; set showToModel:true only if you need to inspect the result. Max 4MB output.',
      'If the terminal can\'t show inline images (VS Code/tmux/plain xterm), the tool saves the PNG and says so — OFFER to open it in a browser and ALWAYS ask the user first (askUser); never open a browser automatically.',
    ],
    parameters: buildQueryEnvelopeSchema(Type, buildParameters(Type), {
      reasoningDescription: 'Concise reason this image creation is necessary.',
    }),

    async execute(toolCallId: string, params: Record<string, unknown>, signal?: AbortSignal, onUpdate?: unknown, ctx?: PiContext): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      return executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        async execute(query, _index, _callId, batchSignal) {
          if (batchSignal?.aborted) throw new Error('Operation aborted');
          const svg = typeof query['svg'] === 'string' ? (query['svg'] as string) : undefined;
          const html = typeof query['html'] === 'string' ? (query['html'] as string) : undefined;
          if (!svg && !html) throw new Error('createImage: provide `svg` or `html`.');
          if (svg && html) throw new Error('createImage: provide only one of `svg` or `html`, not both.');

          const shared = {
            width: typeof query['width'] === 'number' ? (query['width'] as number) : undefined,
            background: typeof query['background'] === 'string' ? (query['background'] as string) : undefined,
            name: typeof query['name'] === 'string' ? (query['name'] as string) : undefined,
            saveTo: typeof query['saveTo'] === 'string' ? (query['saveTo'] as string) : undefined,
          };

          const res = svg
            ? createImageFromSvg(svg, cwd, shared)
            : await createImageFromHtml(html!, cwd, {
                ...shared,
                height: typeof query['height'] === 'number' ? (query['height'] as number) : undefined,
                signal: batchSignal,
              });

          if (!res.ok) throw new Error(res.message);

          // On terminals that can't display inline images (VS Code, tmux, plain
          // xterm, …) the picture won't show. Persist it so the user can open it,
          // and tell the agent to OFFER opening it in a browser — never auto-open.
          const protocolCapable = isTerminalImageCapable();
          const inlineEffective = effectiveInlineImages(ctx);
          const showToModel = query['showToModel'] === true;
          let savedPath = res.savedPath;
          const imagePath = savedPath ?? ((!showToModel || !inlineEffective) && res.base64
            ? persistRenderedPng(res.base64, ctx, res.name)
            : undefined);
          const temporaryArtifact = Boolean(!savedPath && imagePath);
          let message = res.message;
          if (!inlineEffective) {
            if (!savedPath) savedPath = imagePath;
            const reason = protocolCapable
              ? 'inline image display is disabled or unavailable in the current UI mode'
              : 'this terminal has no inline-image support (e.g. VS Code / tmux)';
            const where = savedPath ? ` Saved to ${savedPath}.` : '';
            message = `${res.message} — ${reason}, so it won't render inline here.${where} Offer to open it in a browser; ask the user first, never open automatically.`;
          }

          const content: ToolCallResult['content'] = [{ type: 'text', text: message }];
          if (showToModel) content.unshift({ type: 'image', data: res.base64!, mimeType: 'image/png' });

          return {
            content,
            details: { ok: true, imagePath, mimeType: 'image/png', bytes: res.bytes, name: res.name, savedPath, terminalSupportsImages: protocolCapable, effectiveInlineImages: inlineEffective, temporaryArtifact },
          };
        },
      });
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = args && typeof args === 'object' ? (args as Record<string, unknown>) : {};
      const queries = Array.isArray(envelope['queries']) ? (envelope['queries'] as Record<string, unknown>[]) : [];
      const input = queries[0] ?? {};
      const mode = typeof input['html'] === 'string' ? 'html' : 'svg';
      const name = typeof input['name'] === 'string' ? (input['name'] as string) : mode;
      const title = cliToolTitle(theme, 'createImage');
      return makeRenderer((width) => [truncateToWidth(`${title} ${paint(theme, 'dim', `${mode} \u00b7 ${name}`)}`, width)]);
    },

    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme, context?: RenderContext) {
      if (opts.isPartial) return makeRenderer(() => [paint(theme, 'brand', '… rendering image')]);
      const ok = !result.isError;
      const note = (result.content.find((c) => c.type === 'text') as { text?: string } | undefined)?.text ?? (ok ? 'image created' : 'render failed');
      const icon = paint(theme, cliStatusToken(ok), cliStatusGlyph(ok));
      const base = makeRenderer((width) => [truncateToWidth(`${icon} ${cliToolTitle(theme, 'createImage')} · ${note}`, width)]);
      if (!ok) return base;

      // When showToModel put an image block in content, pi's tool-execution
      // component renders it natively — self-rendering here too would duplicate it.
      if (result.content.some((c) => c.type === 'image')) return base;

      const details = (result.details ?? {}) as { imagePath?: string; name?: string; bytes?: number };
      return details.imagePath
        ? appendImageLines(base, context, details.imagePath, theme, { name: details.name, bytes: details.bytes })
        : base;
    },
  });
}

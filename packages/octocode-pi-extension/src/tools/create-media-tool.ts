/**
 * Artifact-producing half of the public `media` tool. It picks the engine from
 * requested `output` and delegates to existing, separately-tested cores:
 *
 *   type=image                  → resvg / headless Chrome
 *   type=pdf                    → headless Chrome printToPDF
 *   type=gif|trim|audio|convert → ffmpeg / ffprobe
 *
 * Read-only inspection belongs to readMedia.
 *
 * Inline-capable results (image, frame, contactSheet, waveform) render in the
 * TUI like createImage did; the PNG stays out of model context unless
 * showToModel:true. File-producing results (pdf, gif, trim, audio, convert) are
 * written to a path-guarded `dest` and reported.
 */

import fs from 'node:fs';
import path from 'node:path';

import { Marked } from 'marked';

import type { TSchema, ToolCallResult, ToolDefinition, PiContext, PiTheme, RenderContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { buildToolView } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import { resolveFilePath } from './file-state.js';
import { appendImageLines, formatBytes, sniffImageMime } from './image-render.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { createImageFromSvg, createImageFromHtml, persistRenderedPng, renderHtmlToPdf } from './create-image-tool.js';
import { runMediaQuery, type MediaResult } from './media-tool.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

const MEDIA_OPERATIONS = ['image', 'pdf', 'gif', 'trim', 'audio', 'convert', 'concat'] as const;
type MediaOperation = (typeof MEDIA_OPERATIONS)[number];
const FFMPEG_OPERATIONS = new Set<MediaOperation>(['gif', 'trim', 'audio', 'convert', 'concat']);

interface UnifiedResult {
  ok: boolean;
  type: MediaOperation;
  message: string;
  base64?: string;
  mimeType?: string;
  bytes?: number;
  savedPath?: string;
  probe?: MediaResult['probe'];
}

// ---------------------------------------------------------------------------
// PDF composition helpers (pure where possible)
// ---------------------------------------------------------------------------

const PDF_BASE_CSS =
  '@page{margin:18mm}' +
  'html,body{margin:0;padding:0}' +
  'body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;font-size:12pt;line-height:1.5;color:#111}' +
  'h1,h2,h3{line-height:1.25}pre{white-space:pre-wrap;background:#f6f8fa;padding:8px;border-radius:6px;font-size:10pt}' +
  'code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}img{max-width:100%}table{border-collapse:collapse}td,th{border:1px solid #d0d7de;padding:4px 8px}';

/** Wrap fragment HTML (or full doc passthrough) in a print-styled document. */
export function pdfDocumentFromHtml(html: string): string {
  const looksFull = /<html[\s>]/i.test(html) || /<!doctype/i.test(html);
  if (looksFull) return html;
  return `<!doctype html><html><head><meta charset="utf-8"><style>${PDF_BASE_CSS}</style></head><body>${html}</body></html>`;
}

/** Markdown → print-styled HTML document (via marked). */
export function pdfDocumentFromMarkdown(markdown: string): string {
  const body = new Marked().parse(markdown, { async: false }) as string;
  return pdfDocumentFromHtml(body);
}

/** One image per page. Accepts file paths (resolved+guarded) or data: URIs. */
export function pdfDocumentFromImages(dataUris: string[]): string {
  const pages = dataUris
    .map((src, i) => `<div style="page-break-after:${i < dataUris.length - 1 ? 'always' : 'auto'};text-align:center"><img src="${src}" style="max-width:100%;max-height:100vh"></div>`)
    .join('');
  return `<!doctype html><html><head><meta charset="utf-8"><style>@page{margin:0}html,body{margin:0;padding:0}</style></head><body>${pages}</body></html>`;
}

/** Read an image path (guarded) → data: URI. */
function imagePathToDataUri(p: string, cwd: string): string {
  const abs = resolveFilePath(p, cwd);
  assertPathAllowed(abs, cwd, 'media');
  if (!fs.existsSync(abs)) throw new Error(`media: image not found — ${p}`);
  const buf = fs.readFileSync(abs);
  const mime = sniffImageMime(buf) ?? 'image/png';
  return `data:${mime};base64,${buf.toString('base64')}`;
}

/** Resolve + path-guard a destination; refuse to clobber unless overwrite. */
function resolveDest(dest: unknown, cwd: string, overwrite: boolean): string {
  if (typeof dest !== 'string' || dest.trim().length === 0) throw new Error('media: this operation requires `dest`.');
  const abs = resolveFilePath(dest.trim(), cwd);
  assertPathAllowed(abs, cwd, 'media');
  if (fs.existsSync(abs) && !overwrite) throw new Error(`media: \`dest\` exists; set overwrite:true — ${dest}`);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  return abs;
}

// ---------------------------------------------------------------------------
// Core dispatcher (Pi-agnostic)
// ---------------------------------------------------------------------------

export async function runMediaOperation(
  query: Record<string, unknown>,
  cwd: string,
  signal?: AbortSignal,
  deps: {
    createHtmlImage?: typeof createImageFromHtml;
    renderPdf?: typeof renderHtmlToPdf;
    runMedia?: typeof runMediaQuery;
  } = {},
): Promise<UnifiedResult> {
  const type = query['type'] as MediaOperation;
  if (!MEDIA_OPERATIONS.includes(type)) throw new Error(`media: \`type\` must be one of ${MEDIA_OPERATIONS.join(', ')}`);
  const overwrite = query['overwrite'] === true;

  // --- image authoring (resvg / Chrome) ---
  if (type === 'image') {
    const svg = typeof query['svg'] === 'string' ? (query['svg'] as string) : undefined;
    const html = typeof query['html'] === 'string' ? (query['html'] as string) : undefined;
    if (!svg && !html) throw new Error('media: type=image needs `svg` or `html`.');
    if (svg && html) throw new Error('media: provide only one of `svg` or `html`.');
    const shared = {
      width: typeof query['width'] === 'number' ? (query['width'] as number) : undefined,
      background: typeof query['background'] === 'string' ? (query['background'] as string) : undefined,
      name: typeof query['name'] === 'string' ? (query['name'] as string) : undefined,
      saveTo: typeof query['dest'] === 'string' ? (query['dest'] as string) : undefined,
    };
    const res = svg
      ? createImageFromSvg(svg, cwd, shared)
      : await (deps.createHtmlImage ?? createImageFromHtml)(html!, cwd, { ...shared, height: typeof query['height'] === 'number' ? (query['height'] as number) : undefined, signal });
    if (!res.ok) throw new Error(res.message);
    return { ok: true, type, message: res.message, base64: res.base64, mimeType: 'image/png', bytes: res.bytes, savedPath: res.savedPath };
  }

  // --- pdf authoring (Chrome printToPDF) ---
  if (type === 'pdf') {
    const dest = resolveDest(query['dest'], cwd, overwrite);
    const html = typeof query['html'] === 'string' ? (query['html'] as string) : undefined;
    const markdown = typeof query['markdown'] === 'string' ? (query['markdown'] as string) : undefined;
    const images = Array.isArray(query['images']) ? (query['images'] as unknown[]).filter((x): x is string => typeof x === 'string') : undefined;
    const provided = [html, markdown, images && images.length ? 'images' : undefined].filter(Boolean);
    if (provided.length === 0) throw new Error('media: type=pdf needs `html`, `markdown`, or `images`.');
    if (provided.length > 1) throw new Error('media: provide only one PDF source (`html` OR `markdown` OR `images`).');

    let doc: string;
    if (html) doc = pdfDocumentFromHtml(html);
    else if (markdown) doc = pdfDocumentFromMarkdown(markdown);
    else doc = pdfDocumentFromImages(images!.map((p) => imagePathToDataUri(p, cwd)));

    const pdf = await (deps.renderPdf ?? renderHtmlToPdf)(doc, cwd, {
      landscape: query['landscape'] === true,
      scale: typeof query['pdfScale'] === 'number' ? (query['pdfScale'] as number) : undefined,
      signal,
    });
    fs.writeFileSync(dest, pdf);
    return { ok: true, type, savedPath: dest, bytes: pdf.length, message: `wrote ${path.basename(dest)} [application/pdf, ${formatBytes(pdf.length)}]` };
  }

  // --- ffmpeg delegation ---
  if (FFMPEG_OPERATIONS.has(type)) {
    const delegated: Record<string, unknown> = {
      ...query,
      mode: type,
      input: query['source'],
      output: query['dest'],
    };
    delete delegated['dest'];
    delete delegated['source'];
    delete delegated['type'];
    const res = await (deps.runMedia ?? runMediaQuery)(delegated, cwd, signal);
    return {
      ok: res.ok, type, message: res.message, base64: res.base64, mimeType: res.mimeType,
      bytes: res.bytes, savedPath: res.savedPath, probe: res.probe,
    };
  }

  throw new Error(`media: unhandled type ${type}`);
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

function buildParameters(Type: TypeBoxBuilder): TSchema {
  return Type.Object(
    {
      type: Type.Union(MEDIA_OPERATIONS.map((operation) => Type.Literal(operation)), {
        description: 'Operation: image, pdf, gif, trim, audio, convert, or concat.',
      }),
      dest: Type.Optional(Type.String({ description: 'Output path. Required except for inline image.' })),
      overwrite: Type.Optional(Type.Boolean({ description: 'Allow overwriting an existing `dest`. Default false.' })),
      // image / html authoring
      svg: Type.Optional(Type.String({ description: 'image: SVG source.' })),
      html: Type.Optional(Type.String({ description: 'image/pdf: HTML source.' })),
      markdown: Type.Optional(Type.String({ description: 'pdf: Markdown source.' })),
      images: Type.Optional(Type.Array(Type.String(), { description: 'pdf: image paths, one page each.' })),
      width: Type.Optional(Type.Integer({ minimum: 1, maximum: 4096, description: 'Image/GIF/convert width.' })),
      height: Type.Optional(Type.Integer({ minimum: 1, maximum: 4096, description: 'HTML image height.' })),
      background: Type.Optional(Type.String({ description: 'image background color.' })),
      name: Type.Optional(Type.String({ description: 'image display name.' })),
      landscape: Type.Optional(Type.Boolean({ description: 'pdf landscape mode.' })),
      pdfScale: Type.Optional(Type.Unsafe({ type: 'number', minimum: 0.1, maximum: 2, description: 'pdf scale; default 1.' })),
      // ffmpeg passthrough
      source: Type.Optional(Type.String({ description: 'Input path for gif/trim/audio/convert.' })),
      sources: Type.Optional(Type.Array(Type.String(), { minItems: 2, description: 'concat: ordered list of input paths to join (minimum 2).' })),
      from: Type.Optional(Type.String({ description: 'gif/trim: start timestamp.' })),
      to: Type.Optional(Type.String({ description: 'gif/trim: end timestamp.' })),
      duration: Type.Optional(Type.String({ description: 'trim: clip length (alternative to `to`).' })),
      reencode: Type.Optional(Type.Boolean({ description: 'trim/concat: frame-accurate re-encode instead of fast stream-copy.' })),
      fps: Type.Optional(Type.Integer({ minimum: 1, description: 'gif/convert: frames per second.' })),
      format: Type.Optional(Type.String({ description: 'audio: mp3 | aac | wav | flac.' })),
      bitrate: Type.Optional(Type.String({ description: 'audio: e.g. "192k". convert: target bitrate for hw codecs (h264_videotoolbox/hevc_videotoolbox), e.g. "4M".' })),
      scale: Type.Optional(Type.String({ description: 'convert: WxH, e.g. "1280x-1".' })),
      videoCodec: Type.Optional(Type.String({ description: 'convert/trim/concat: h264 | hevc | vp9 | av1 | copy | h264_videotoolbox | hevc_videotoolbox (hw, macOS).' })),
      audioCodec: Type.Optional(Type.String({ description: 'convert/trim/concat: aac | mp3 | copy | none.' })),
      crf: Type.Optional(Type.Integer({ minimum: 0, maximum: 51, description: 'convert: quality (lower=better, 23 default).' })),
      timeoutSec: Type.Optional(Type.Integer({ minimum: 1, description: 'ffmpeg modes: max seconds before the process is killed. Default 120.' })),
      showToModel: Type.Optional(Type.Boolean({ description: 'image: also return pixels to model.' })),
    },
    { additionalProperties: false },
  );
}

export function registerMediaTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'media',
    label: 'Media',
    description: 'Create or transform media. Render image/PDF from SVG, HTML, Markdown, or images; make GIFs, trim clips, extract audio, convert formats, or concat sources[]. Writes are path-guarded; use readMedia for inspection.',
    promptSnippet: 'Create image/PDF artifacts or transform existing audio/video/image files.',
    promptGuidelines: [
      'Use type:image/pdf to author; type:gif/trim/audio/convert transforms `source` into `dest`.',
      'type:concat joins sources[] — reencode:true for different codecs/resolutions.',
      'convert videoCodec:"h264_videotoolbox"/"hevc_videotoolbox" for hardware encoding on macOS.',
      'Use readMedia for metadata, frames, contact sheets, waveforms, and spectrograms.',
    ],
    parameters: buildQueryEnvelopeSchema(Type, buildParameters(Type), {
      reasoningDescription: 'Concise reason this media operation is necessary.',
    }),

    async execute(toolCallId: string, params: Record<string, unknown>, signal?: AbortSignal, onUpdate?: unknown, ctx?: PiContext): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      return executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate: typeof onUpdate === 'function' ? (onUpdate as (u: ToolCallResult) => void) : undefined,
        ctx,
        passthroughSingle: true,
        async execute(query, _index, _callId, batchSignal) {
          if (batchSignal?.aborted) throw new Error('Operation aborted');
          const res = await runMediaOperation(query, cwd, batchSignal);
          if (!res.ok) throw new Error(res.message);

          const content: ToolCallResult['content'] = [{ type: 'text', text: res.message }];
          if (query['showToModel'] === true && res.base64) {
            content.unshift({ type: 'image', data: res.base64, mimeType: res.mimeType ?? 'image/png' });
          }
          const imagePath = res.savedPath ?? (query['showToModel'] !== true && res.base64
            ? persistRenderedPng(res.base64, ctx, typeof query['name'] === 'string' ? query['name'] : `${res.type}.png`)
            : undefined);
          return {
            content,
            details: {
              ok: true, type: res.type, imagePath, mimeType: res.mimeType, bytes: res.bytes,
              savedPath: res.savedPath, probe: res.probe,
            },
          };
        },
      });
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = args && typeof args === 'object' ? (args as Record<string, unknown>) : {};
      const queries = Array.isArray(envelope['queries']) ? (envelope['queries'] as Record<string, unknown>[]) : [];
      const input = queries[0] ?? {};
      const operation = typeof input['type'] === 'string' ? (input['type'] as string) : 'media';
      const hint = typeof input['source'] === 'string' ? path.basename(input['source'] as string)
        : typeof input['name'] === 'string' ? (input['name'] as string) : '';
      return buildToolView({ name: 'media', state: 'request', segments: [{ text: operation, token: 'bright' }, ...(hint ? [{ text: hint, token: 'path' as const }] : [])] }, theme);
    },

    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme, context?: RenderContext) {
      if (opts.isPartial) return buildToolView(() => ({ name: 'media', state: 'running', status: 'processing…' }), theme);
      const ok = !result.isError;
      const note = (result.content.find((c) => c.type === 'text') as { text?: string } | undefined)?.text ?? (ok ? 'done' : 'failed');
      const details = (result.details ?? {}) as { imagePath?: string; savedPath?: string };
      const base = buildToolView({
        name: 'media',
        state: ok ? 'success' : 'error',
        segments: [
          ...(details.savedPath ? [{ text: details.savedPath, token: 'path' as const }] : []),
          { text: note.split('\n').find(Boolean) ?? note, token: ok ? 'dim' : 'error' },
        ],
      }, theme);
      if (!ok) return base;
      if (result.content.some((c) => c.type === 'image')) return base;

      return details.imagePath ? appendImageLines(base, context, details.imagePath, theme) : base;
    },
  });
}

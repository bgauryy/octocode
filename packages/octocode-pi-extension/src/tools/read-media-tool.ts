import path from 'node:path';
import { buildToolView } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import { resolveFilePath } from './file-state.js';
import { buildImageLinesFromData, effectiveInlineImages, formatBytes, isTerminalImageCapable, loadImageForRender, terminalImageProtocol } from './image-render.js';
import { runMediaQuery } from './media-tool.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import type { TSchema, ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;
type MediaType = 'image' | 'video' | 'audio';
type MediaView = 'metadata' | 'frame' | 'contactSheet' | 'waveform' | 'spectrogram';

export interface ReadMediaImageResult {
  ok: boolean;
  message: string;
  mimeType?: string;
  base64?: string;
  bytes?: number;
}

export function readMediaImageFile(filePath: string, cwd: string): ReadMediaImageResult {
  const abs = resolveFilePath(filePath, cwd);
  assertPathAllowed(abs, cwd, 'readMedia');
  const loaded = loadImageForRender(abs);
  if (!loaded) {
    return {
      ok: false,
      message: `Cannot read image "${filePath}": missing, empty, over 4MB, or not png/jpeg/gif/webp.`,
    };
  }
  const bytes = Math.floor(loaded.base64.length * 3 / 4);
  return {
    ok: true,
    message: `Read image ${path.basename(abs)} [${loaded.mimeType}, ${formatBytes(bytes)}]`,
    mimeType: loaded.mimeType,
    base64: loaded.base64,
    bytes,
  };
}

function resolveView(type: MediaType, requested: unknown): MediaView {
  const view = typeof requested === 'string' ? requested as MediaView : undefined;
  if (type === 'video') {
    const resolved = view ?? 'contactSheet';
    if (!['metadata', 'frame', 'contactSheet'].includes(resolved)) {
      throw new Error('readMedia: video view must be metadata, frame, or contactSheet.');
    }
    return resolved;
  }
  const resolved = view ?? 'waveform';
  if (!['metadata', 'waveform', 'spectrogram'].includes(resolved)) {
    throw new Error('readMedia: audio view must be metadata, waveform, or spectrogram.');
  }
  return resolved;
}

function buildParameters(Type: TypeBoxBuilder): TSchema {
  return Type.Object({
    type: Type.Union(
      [Type.Literal('image'), Type.Literal('video'), Type.Literal('audio')],
      { description: 'Media kind. image returns pixels; video/audio default to a visual summary.' },
    ),
    path: Type.String({ minLength: 1, description: 'Local media path.' }),
    view: Type.Optional(Type.Union(
      ['metadata', 'frame', 'contactSheet', 'waveform', 'spectrogram'].map((value) => Type.Literal(value)),
      { description: 'video: metadata/frame/contactSheet. audio: metadata/waveform/spectrogram.' },
    )),
    at: Type.Optional(Type.String({ description: 'frame timestamp; default 0.' })),
    count: Type.Optional(Type.Integer({ minimum: 1, maximum: 64, description: 'contactSheet frame count; default 9.' })),
    columns: Type.Optional(Type.Integer({ minimum: 1, maximum: 64, description: 'contactSheet columns.' })),
    width: Type.Optional(Type.Integer({ minimum: 1, maximum: 4096, description: 'Visual width in pixels.' })),
    height: Type.Optional(Type.Integer({ minimum: 1, maximum: 4096, description: 'waveform/spectrogram height.' })),
    timeoutSec: Type.Optional(Type.Integer({ minimum: 1, maximum: 1800, description: 'ffmpeg timeout; default 120.' })),
  }, { additionalProperties: false }) as TSchema;
}

export function registerReadMediaTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'readMedia',
    label: 'Read Media',
    description: 'Read local media into model context. image returns pixels; video returns metadata, a frame, or a contact sheet; audio returns metadata, a waveform, or a spectrogram. Read-only—use media to create or transform files.',
    promptSnippet: 'Inspect local image/video/audio content; use media for authored or transformed outputs.',
    promptGuidelines: [
      'Use type:image for screenshots/diagrams, type:video for a frame/contact sheet, and type:audio for a waveform/spectrogram.',
      'Use view:metadata when visual content is unnecessary. Video/audio views require ffmpeg.',
    ],
    parameters: buildQueryEnvelopeSchema(Type, buildParameters(Type), {
      reasoningDescription: 'Why this media must be inspected.',
      allowParallel: true,
    }),

    async execute(toolCallId, params, signal, onUpdate, ctx): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      return executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        allowParallel: true,
        async execute(query, _index, _callId, batchSignal) {
          if (batchSignal?.aborted) throw new Error('Operation aborted');
          const type = query['type'] as MediaType;
          if (!['image', 'video', 'audio'].includes(type)) {
            throw new Error('readMedia: `type` must be image, video, or audio.');
          }
          const filePath = query['path'];
          if (typeof filePath !== 'string' || filePath.length === 0) {
            throw new Error('readMedia: `path` is required.');
          }

          if (type === 'image') {
            const res = readMediaImageFile(filePath, cwd);
            if (!res.ok) throw new Error(res.message);
            const protocolCapable = isTerminalImageCapable();
            const protocol = terminalImageProtocol();
            const inlineEffective = effectiveInlineImages(ctx);
            const absPath = resolveFilePath(filePath, cwd);
            const note = inlineEffective
              ? res.message
              : `${res.message} — inline display is unavailable in this TUI. Offer to open ${absPath} in the user's browser; ask the user first.`;
            return {
              content: [
                { type: 'image', data: res.base64!, mimeType: res.mimeType! },
                { type: 'text', text: note },
              ],
              details: {
                ok: true,
                type,
                mimeType: res.mimeType,
                bytes: res.bytes,
                sourcePath: absPath,
                terminalSupportsImages: protocolCapable,
                terminalImageProtocol: protocol,
                effectiveInlineImages: inlineEffective,
              },
            };
          }

          const view = resolveView(type, query['view']);
          const mode = view === 'metadata' ? 'probe' : view === 'spectrogram' ? 'waveform' : view;
          const res = await runMediaQuery({
            ...query,
            mode,
            input: filePath,
            kind: view === 'spectrogram' ? 'spectrogram' : view === 'waveform' ? 'waveform' : undefined,
          }, cwd, batchSignal);
          const content: ToolCallResult['content'] = [{ type: 'text', text: res.message }];
          if (res.base64 && res.mimeType) {
            content.unshift({ type: 'image', data: res.base64, mimeType: res.mimeType });
          }
          return {
            content,
            details: {
              ok: true,
              type,
              view,
              mimeType: res.mimeType,
              bytes: res.bytes,
              sourcePath: resolveFilePath(filePath, cwd),
              probe: res.probe,
            },
          };
        },
      });
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = (args ?? {}) as Record<string, unknown>;
      const queries = Array.isArray(envelope['queries']) ? envelope['queries'] as Record<string, unknown>[] : [];
      const input = queries[0] ?? {};
      const type = typeof input['type'] === 'string' ? input['type'] : 'media';
      const filePath = typeof input['path'] === 'string' ? input['path'] : '(missing path)';
      return buildToolView({ name: 'readMedia', state: 'request', segments: [{ text: type, token: 'bright' }, { text: filePath, token: 'path' }] }, theme);
    },

    renderResult(result, opts, theme, context) {
      if (opts.isPartial) return buildToolView(() => ({ name: 'readMedia', state: 'running', status: 'reading…' }), theme);
      const ok = !result.isError;
      const note = (result.content.find((c) => c.type === 'text') as { text?: string } | undefined)?.text
        ?? (ok ? 'media loaded' : 'read failed');
      const details = result.details && typeof result.details === 'object' ? result.details as Record<string, unknown> : {};
      const source = typeof details['sourcePath'] === 'string' ? details['sourcePath'] : '';
      const base = buildToolView({
        name: 'readMedia',
        state: ok ? 'success' : 'error',
        segments: [
          { text: note.split('\n').find(Boolean) ?? note, token: ok ? 'dim' : 'error' },
          ...(source ? [{ text: source, token: 'path' as const }] : []),
        ],
      }, theme);
      if (!opts.expanded) return base;
      const image = result.content.find((part) => part.type === 'image') as { data?: string; mimeType?: string } | undefined;
      if (!image?.data || !image.mimeType) return base;
      const sourcePath = typeof details['sourcePath'] === 'string' ? details['sourcePath'] : 'read-media-image';
      const name = path.basename(sourcePath) || 'media-preview';
      const bytes = typeof details['bytes'] === 'number' ? details['bytes'] : undefined;
      return {
        render(width = 80) {
          return [
            ...base.render(width),
            ...buildImageLinesFromData(context, sourcePath, image.data!, image.mimeType!, width, { theme, name, bytes }),
          ];
        },
        invalidate() { base.invalidate(); },
      };
    },
  } satisfies ToolDefinition);
}

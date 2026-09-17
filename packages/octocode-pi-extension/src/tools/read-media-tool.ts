import path from 'node:path';
import { buildToolView } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import { resolveFilePath } from './file-state.js';
import { buildImageLinesFromData, effectiveInlineImages, formatBytes, isTerminalImageCapable, loadImageForRender, terminalImageProtocol } from './image-render.js';
import { runMediaQuery } from './media-tool.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import type { ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';

import { z } from 'zod';
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
  assertPathAllowed(abs, cwd, 'inspectMedia');
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
      throw new Error('inspectMedia: video view must be metadata, frame, or contactSheet.');
    }
    return resolved;
  }
  const resolved = view ?? 'waveform';
  if (!['metadata', 'waveform', 'spectrogram'].includes(resolved)) {
    throw new Error('inspectMedia: audio view must be metadata, waveform, or spectrogram.');
  }
  return resolved;
}

const readMediaItemSchema = z.looseObject({
  type: z.enum(['image', 'video', 'audio']).describe(
    'Media kind. image returns pixels; video/audio default to a visual summary.',
  ),
  path: z.string().min(1).describe('Local media path.'),
  view: z.enum(['metadata', 'frame', 'contactSheet', 'waveform', 'spectrogram']).optional()
    .describe('video: metadata/frame/contactSheet. audio: metadata/waveform/spectrogram.'),
  at: z.string().optional().describe('frame timestamp; default 0.'),
  count: z.number().int().min(1).max(64).optional().describe('contactSheet frame count; default 9.'),
  columns: z.number().int().min(1).max(64).optional().describe('contactSheet columns.'),
  width: z.number().int().min(1).max(4096).optional().describe('Visual width in pixels.'),
  height: z.number().int().min(1).max(4096).optional().describe('waveform/spectrogram height.'),
  timeoutSec: z.number().int().min(1).max(1800).optional().describe('ffmpeg timeout; default 120.'),
});

export function registerReadMediaTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'inspectMedia',
    label: 'Inspect Media',
    description: DIRECT_TOOL_DESCRIPTIONS.inspectMedia!,
    promptSnippet: 'Inspect local pixels, media metadata, or visual summaries.',
    promptGuidelines: [
      'Use type:image for screenshots/diagrams (returns inline pixels to the model for vision); type:video for a frame or contact sheet; type:audio for waveform/spectrogram.',
      'Use view:metadata for video/audio when visual content is unnecessary; image inspection returns pixels.',
    ],
    parameters: buildQueryEnvelopeSchema(readMediaItemSchema, { allowParallel: true }),

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
        preflight(query) {
          const parsed = readMediaItemSchema.parse(query);
          assertPathAllowed(resolveFilePath(parsed.path, cwd), cwd, 'inspectMedia');
          if (parsed.type !== 'image') resolveView(parsed.type, parsed.view);
        },
        async execute(query, _index, _callId, batchSignal) {
          if (batchSignal?.aborted) throw new Error('Operation aborted');
          const type = query['type'] as MediaType;
          if (!['image', 'video', 'audio'].includes(type)) {
            throw new Error('inspectMedia: `type` must be image, video, or audio.');
          }
          const filePath = query['path'];
          if (typeof filePath !== 'string' || filePath.length === 0) {
            throw new Error('inspectMedia: `path` is required.');
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
      return buildToolView({ name: 'inspectMedia', state: 'request', segments: [{ text: type, token: 'bright' }, { text: filePath, token: 'path' }] }, theme);
    },

    renderResult(result, opts, theme, context) {
      if (opts.isPartial) return buildToolView(() => ({ name: 'inspectMedia', state: 'running', status: 'reading…' }), theme);
      const ok = !result.isError;
      const note = (result.content.find((c) => c.type === 'text') as { text?: string } | undefined)?.text
        ?? (ok ? 'media loaded' : 'read failed');
      const details = result.details && typeof result.details === 'object' ? result.details as Record<string, unknown> : {};
      const source = typeof details['sourcePath'] === 'string' ? details['sourcePath'] : '';
      const base = buildToolView({
        name: 'inspectMedia',
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

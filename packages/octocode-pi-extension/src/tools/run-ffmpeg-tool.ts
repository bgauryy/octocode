/**
 * run-ffmpeg-tool — direct, path-guarded ffmpeg/ffprobe runner.
 *
 * Exposes full ffmpeg power (filter_complex, avfoundation, loudnorm, VMAF, …)
 * without a shell. Every file argument is resolved and path-guarded before the
 * process starts; timeout + AbortSignal apply; progress lines are forwarded.
 *
 * Use this when inspectMedia / media don't cover the operation:
 *   - Side-by-side / stacked   -filter_complex hstack / vstack
 *   - Text overlay             -vf drawtext=…  (needs --enable-libfreetype)
 *   - Audio normalize          -af loudnorm=I=-16:TP=-1.5:LRA=11
 *   - Silence detection        -af silencedetect=noise=-30dB:d=0.5 -f null -
 *   - Speed change             -vf setpts=0.5*PTS -af atempo=2.0
 *   - Screen recording         -f avfoundation -i "1" -t 10
 *   - VMAF quality score       -lavfi "[0:v][1:v]libvmaf=log_path=vmaf.json" -f null -
 *   - ProRes master            -c:v prores_videotoolbox -profile:v 3
 *   - Complex concat           -filter_complex "[0:v][0:a][1:v][1:a]concat=…"
 *
 * Common transforms (HW encode, concat, gif, trim, convert): use media instead.
 * Reference + cookbook: docs/FFMPEG.md
 */

import { detectFfmpeg, runBinary } from './ffmpeg-runtime.js';
import { assertPathAllowed } from './path-guard.js';
import { resolveFilePath } from './file-state.js';
/** Clamp + coerce an integer param — mirrors the private helper in media-tool.ts. */
function clampInt(val: unknown, min: number, max: number, def: number): number {
  const n = typeof val === 'number' ? val : typeof val === 'string' ? Number(val) : NaN;
  return isNaN(n) ? def : Math.max(min, Math.min(max, Math.round(n)));
}
import { buildToolView } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { createSessionArtifactContext } from './session-artifacts.js';
import type { ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;

export function persistFfmpegStdout(
  stdout: Buffer,
  cwd: string,
  ctx: Parameters<typeof createSessionArtifactContext>[0] | undefined,
  itemCallId: string,
): string {
  const artifacts = createSessionArtifactContext(ctx ?? { cwd });
  const safeCallId = itemCallId.replace(/[^a-zA-Z0-9._-]+/g, '-').slice(0, 96) || 'ffmpeg';
  const relative = `tool-results/${safeCallId}-stdout.bin`;
  artifacts.writeBinary(relative, stdout);
  artifacts.registerProducer('log', relative);
  return artifacts.resolve(relative);
}

/**
 * Resolve and path-guard any argv entry that looks like an absolute or relative
 * file path. Flags and non-path arguments (codec names, filter expressions,
 * numeric values, etc.) are left untouched.
 *
 * Strategy:
 *   1. If an arg immediately follows a file-bearing flag (-i), resolve it.
 *   2. If an arg looks like a path (contains / or starts with . or ~) and
 *      does NOT look like a filter expression or flag, resolve it.
 *   3. The last non-flag, non-numeric arg in the list is treated as the
 *      output path if it contains a file extension.
 */
function resolveArgvPaths(args: string[], cwd: string): string[] {
  const resolved: string[] = [];
  let nextIsFile = false;

  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;

    if (nextIsFile) {
      // This arg is a file path following a recognised file flag.
      // Skip resolution for pipe targets (-) and protocol URLs.
      if (arg !== '-' && !arg.includes('://') && !arg.startsWith('[')) {
        const abs = resolveFilePath(arg, cwd);
        // Allow both existing files (inputs) and new paths (outputs).
        // Only assert allowed — the actual existence check is left to ffmpeg
        // so we don't reject valid output paths that don't exist yet.
        assertPathAllowed(abs, cwd, 'runFfmpeg');
        resolved.push(abs);
      } else {
        resolved.push(arg);
      }
      nextIsFile = false;
      continue;
    }

    // Flags whose NEXT argument is a file path (inputs, outputs, and log files).
    // -progress/-passlogfile/-vstats_file/-sdp_file take file paths and must be path-guarded.
    if (arg === '-i' || arg === '-o' || arg === '-progress' || arg === '-passlogfile' ||
        arg === '-vstats_file' || arg === '-sdp_file') {
      nextIsFile = true;
      resolved.push(arg);
      continue;
    }

    // Heuristic: resolve args that look like file paths (not flags or filter exprs).
    // Key exclusion: args containing '=' are filter-graph expressions (movie=input.mp4,
    // log_path=vmaf.json, key=value) - never bare file paths. Without this guard a
    // filter string ending in an extension (e.g. `movie=input.mp4`) would be wrongly
    // resolved to an absolute path and break the command.
    const looksLikePath =
      !arg.startsWith('-') &&
      !arg.startsWith('[') &&
      !arg.includes('=') &&
      (arg.startsWith('./') || arg.startsWith('../') || arg.startsWith('/') ||
        // relative paths with a file extension that aren't pure numbers or codec names
        (/[./]/.test(arg) && /\.[a-zA-Z0-9]{2,4}$/.test(arg) && !/^[\d.]+$/.test(arg)));

    if (looksLikePath) {
      const abs = resolveFilePath(arg, cwd);
      assertPathAllowed(abs, cwd, 'runFfmpeg');
      resolved.push(abs);
    } else {
      resolved.push(arg);
    }
  }

  return resolved;
}

const runFfmpegItemSchema = z.looseObject({
  binary: z.enum(['ffmpeg', 'ffprobe'])
    .describe('Which binary to run. Default: ffmpeg. Use ffprobe for metadata-only queries.')
    .optional(),
  args: z.array(z.string()).min(1).describe(
    'Argv without binary or shell syntax, e.g. ["-i","input.mp4","out.mp4"]. Paths resolve against cwd; overwrite flags require authorized replacement.',
  ),
  captureStdout: z.boolean().optional().describe(
    'Capture pipe-to-stdout image/data bytes in a private session artifact and return its path. ' +
    'Default false — stdout is ignored and only stderr/exit-code matter.',
  ),
  timeoutSec: z.number().int().min(1).max(1800).optional()
    .describe('Wall-clock timeout in seconds. Default 120.'),
});

export function registerRunFfmpegTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'runFfmpeg',
    label: 'Run FFmpeg',
    description:
      DIRECT_TOOL_DESCRIPTIONS.runFfmpeg!,
    promptSnippet:
      'Run advanced ffmpeg operations outside the media presets.',
    promptGuidelines: [
      'args = argv without binary name. Both get -hide_banner; ffmpeg gets -nostdin. Paths auto-resolved.',
      'Default fresh dest; -y only for authorized overwrite (stdin disabled without it).',
      'ffprobe auto-captures stdout. captureStdout:true only for ffmpeg binary-to-stdout (output arg "-").',
      'Use docs/FFMPEG.md#cookbook when a recipe is needed.',
    ],
    parameters: buildQueryEnvelopeSchema(runFfmpegItemSchema, {
      reasoningDescription: 'Why this ffmpeg command is needed and what it produces.',
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
        preflight(query) {
          const parsed = runFfmpegItemSchema.parse(query);
          resolveArgvPaths(parsed.args, cwd);
        },
        async execute(query, _index, itemCallId, batchSignal) {
          if (batchSignal?.aborted) throw new Error('Operation aborted');

          const det = detectFfmpeg();
          if (!det.ok) throw new Error(det.reason ?? 'ffmpeg unavailable');

          const binaryName = (query['binary'] as string | undefined) ?? 'ffmpeg';
          const bin = binaryName === 'ffprobe' ? det.ffprobe! : det.ffmpeg!;

          const rawArgs = query['args'];
          if (!Array.isArray(rawArgs) || rawArgs.length === 0) {
            throw new Error('runFfmpeg: `args` must be a non-empty string array.');
          }
          const stringArgs = rawArgs.map((a: unknown) => {
            if (typeof a !== 'string') throw new Error(`runFfmpeg: args must be strings, got ${typeof a}`);
            return a;
          });

          const resolvedArgs = resolveArgvPaths(stringArgs, cwd);
          const timeoutMs = clampInt(query['timeoutSec'], 1, 1800, 120)! * 1000;
          // ffprobe writes its output (JSON/text) to stdout, not stderr.
          // Auto-enable captureStdout for ffprobe so output is never silently lost
          // or rejected with a misleading 'exceeded 0MB' error.
          const captureStdout = query['captureStdout'] === true || binaryName === 'ffprobe';

          // ffprobe has no -nostdin option; runBinary closes stdin for both.
          const finalArgs = ['-hide_banner', ...(binaryName === 'ffmpeg' ? ['-nostdin'] : []), ...resolvedArgs];

          const result = await runBinary(bin, finalArgs, {
            cwd,
            signal: batchSignal,
            timeoutMs,
            maxStdoutBytes: captureStdout ? 32 * 1024 * 1024 : 0,
            onProgress: typeof onUpdate === 'function'
              ? (fields) => {
                  const time = fields['out_time'] ?? '';
                  const speed = fields['speed'] ?? '';
                  const note = [time && `time=${time}`, speed && `speed=${speed}`]
                    .filter(Boolean).join(' ');
                  if (note) {
                    (onUpdate as (u: ToolCallResult) => void)({
                      content: [{ type: 'text', text: `⏳ ffmpeg ${note}` }],
                    });
                  }
                }
              : undefined,
          });

          if (result.aborted) throw new Error('runFfmpeg: operation aborted.');
          if (result.code !== 0) {
            throw new Error(
              `runFfmpeg: exited with code ${result.code}.\n` +
              result.stderr.slice(-2000),
            );
          }

          const message = [
            `ffmpeg exited 0`,
            result.stderr.split('\n').find(l => /encoded|muxing overhead|video:|audio:/.test(l))?.trim(),
          ].filter(Boolean).join(' — ');

          const content: ToolCallResult['content'] = [{ type: 'text', text: message }];

          if (captureStdout && result.stdout.length > 0) {
            const artifact = persistFfmpegStdout(result.stdout, cwd, ctx, itemCallId);
            content.push({ type: 'text', text: `stdout (${result.stdout.length} bytes): ${artifact}` });
          }

          return {
            content,
            details: {
              ok: true,
              binary: binaryName,
              args: resolvedArgs,
              exitCode: result.code,
              stdoutBytes: result.stdout.length,
              stderrTail: result.stderr.slice(-500),
            },
          };
        },
      });
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = (args ?? {}) as Record<string, unknown>;
      const queries = Array.isArray(envelope['queries']) ? envelope['queries'] as Record<string, unknown>[] : [];
      const input = queries[0] ?? {};
      const binary = typeof input['binary'] === 'string' ? input['binary'] : 'ffmpeg';
      const argv = Array.isArray(input['args']) ? (input['args'] as string[]).join(' ') : '(no args)';
      return buildToolView({
        name: 'runFfmpeg',
        state: 'request',
        segments: [{ text: binary, token: 'bright' }, { text: argv, token: 'dim' }],
      }, theme);
    },

    renderResult(result, opts, theme) {
      if (opts.isPartial) return buildToolView(() => ({ name: 'runFfmpeg', state: 'running', status: 'encoding…' }), theme);
      const ok = !result.isError;
      const note = (result.content.find((c) => c.type === 'text') as { text?: string } | undefined)?.text
        ?? (ok ? 'done' : 'failed');
      return buildToolView({
        name: 'runFfmpeg',
        state: ok ? 'success' : 'error',
        segments: [{ text: note.split('\n').find(Boolean) ?? (ok ? 'done' : 'failed'), token: ok ? 'dim' : 'error' }],
      }, theme);
    },
  } satisfies ToolDefinition);
}

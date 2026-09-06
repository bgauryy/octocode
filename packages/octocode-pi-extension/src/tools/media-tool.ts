import { extensionWorkspaceRoot } from '../extension-paths.js';
/**
 * ffmpeg/ffprobe core shared by readMedia and media.
 *
 * Modes:
 *   • probe        — ffprobe metadata (codec, duration, resolution, streams)
 *   • frame        — single frame at a timestamp → inline PNG
 *   • contactSheet — N frames tiled into one grid PNG → inline preview
 *   • waveform     — audio waveform or spectrogram → inline PNG
 *   • gif          — video → palette-optimized GIF / animated WebP (file out)
 *   • trim         — clip between timestamps (stream-copy by default) (file out)
 *   • audio        — extract audio track (mp3/aac/wav/flac) (file out)
 *   • convert      — transcode / resize / re-fps (file out)
 *
 * readMedia owns read modes; the public media tool owns produce modes.
 *
 * Security: argv is built explicitly and run WITHOUT a shell (see ffmpeg-runtime),
 * so user paths/timestamps can never inject flags. Every input/output path is
 * checked with assertPathAllowed. Timestamps and numbers are validated up front.
 */

import fs from 'node:fs';
import path from 'node:path';

import { assertPathAllowed } from './path-guard.js';
import { resolveFilePath } from './file-state.js';
import { formatBytes } from './image-render.js';
import { detectFfmpeg, runFfmpeg, runFfprobeJson } from './ffmpeg-runtime.js';


const MODES = ['probe', 'frame', 'contactSheet', 'waveform', 'gif', 'trim', 'audio', 'convert', 'concat'] as const;
type Mode = (typeof MODES)[number];
const IMAGE_MODES = new Set<Mode>(['frame', 'contactSheet', 'waveform']);

const MAX_DIMENSION = 4096;
const MAX_TILES = 64;
export const MAX_MODEL_MEDIA_IMAGE_BYTES = 4 * 1024 * 1024;

export interface ProbeSummary {
  format?: string;
  durationSec?: number;
  width?: number;
  height?: number;
  fps?: number;
  videoCodec?: string;
  audioCodec?: string;
  bitrate?: number;
}

export interface MediaResult {
  ok: boolean;
  message: string;
  mode: Mode;
  /** Image modes: PNG payload. */
  base64?: string;
  bytes?: number;
  mimeType?: string;
  /** Produce modes: written file. */
  savedPath?: string;
  probe?: ProbeSummary;
}

// ---------------------------------------------------------------------------
// Validation helpers (pure, unit-testable)
// ---------------------------------------------------------------------------

/** Accept `SS`, `SS.mmm`, `MM:SS`, `HH:MM:SS`, `HH:MM:SS.mmm`. */
export function isValidTimestamp(ts: string): boolean {
  return /^(\d+(\.\d+)?|(\d{1,2}:)?[0-5]?\d:[0-5]?\d(\.\d+)?)$/.test(ts.trim());
}

function requireTimestamp(ts: unknown, field: string): string {
  if (typeof ts !== 'string' || !isValidTimestamp(ts)) {
    throw new Error(`media: \`${field}\` must be a timestamp like "12", "1:05" or "00:01:05.500"`);
  }
  return ts.trim();
}

function clampInt(value: unknown, min: number, max: number, fallback?: number): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return Math.min(Math.max(min, Math.floor(value)), max);
}

/** Resolve + path-guard an input path that must already exist. */
function resolveInput(input: unknown, cwd: string): string {
  if (typeof input !== 'string' || input.trim().length === 0) throw new Error('media: `input` path is required.');
  const abs = resolveFilePath(input.trim(), cwd);
  assertPathAllowed(abs, cwd, 'media');
  if (!fs.existsSync(abs)) throw new Error(`media: input not found — ${input}`);
  return abs;
}

/** Resolve + path-guard an output path; refuse to clobber unless overwrite. */
function resolveOutput(output: unknown, cwd: string, overwrite: boolean): string {
  if (typeof output !== 'string' || output.trim().length === 0) throw new Error('media: this mode requires an `output` path.');
  const abs = resolveFilePath(output.trim(), cwd);
  assertPathAllowed(abs, cwd, 'media');
  if (fs.existsSync(abs) && !overwrite) throw new Error(`media: \`output\` already exists (set overwrite:true) — ${output}`);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  return abs;
}

// ---------------------------------------------------------------------------
// ffprobe summary
// ---------------------------------------------------------------------------

function parseFrameRate(rate: unknown): number | undefined {
  if (typeof rate !== 'string') return undefined;
  const [num, den] = rate.split('/').map(Number);
  if (!num || !den) return undefined;
  return Math.round((num / den) * 100) / 100;
}

export function summarizeProbe(json: Record<string, unknown>): ProbeSummary {
  const streams = Array.isArray(json['streams']) ? (json['streams'] as Record<string, unknown>[]) : [];
  const format = (json['format'] ?? {}) as Record<string, unknown>;
  const video = streams.find((s) => s['codec_type'] === 'video');
  const audio = streams.find((s) => s['codec_type'] === 'audio');
  const durationSec = Number(format['duration']) || Number(video?.['duration']) || undefined;
  const bitrate = Number(format['bit_rate']) || undefined;
  return {
    format: typeof format['format_name'] === 'string' ? (format['format_name'] as string) : undefined,
    durationSec: durationSec ? Math.round(durationSec * 100) / 100 : undefined,
    width: video ? Number(video['width']) || undefined : undefined,
    height: video ? Number(video['height']) || undefined : undefined,
    fps: video ? parseFrameRate(video['avg_frame_rate'] ?? video['r_frame_rate']) : undefined,
    videoCodec: video ? (video['codec_name'] as string | undefined) : undefined,
    audioCodec: audio ? (audio['codec_name'] as string | undefined) : undefined,
    bitrate,
  };
}

function describeProbe(p: ProbeSummary): string {
  const bits: string[] = [];
  if (p.width && p.height) bits.push(`${p.width}×${p.height}`);
  if (p.videoCodec) bits.push(p.videoCodec);
  if (p.audioCodec) bits.push(`+${p.audioCodec}`);
  if (p.fps) bits.push(`${p.fps}fps`);
  if (p.durationSec !== undefined) bits.push(`${p.durationSec}s`);
  if (p.bitrate) bits.push(`${Math.round(p.bitrate / 1000)}kbps`);
  return bits.join(' ');
}

// ---------------------------------------------------------------------------
// argv builders (pure) — one per mode
// ---------------------------------------------------------------------------

/** Extract a single frame to a temp PNG path. */
export function frameArgs(input: string, out: string, at: string, width?: number): string[] {
  const args = ['-y', '-ss', at, '-i', input, '-frames:v', '1'];
  if (width) args.push('-vf', `scale=${width}:-1`);
  args.push(out);
  return args;
}

/** Tile `count` evenly-sampled frames into a `columns`-wide grid PNG. */
export function contactSheetArgs(
  input: string, out: string, durationSec: number, count: number, columns: number, tileWidth: number,
): string[] {
  const rows = Math.ceil(count / columns);
  // Sample `count` frames across the whole clip; guard against zero-duration.
  const rate = durationSec > 0 ? count / durationSec : count;
  const vf = `fps=${rate.toFixed(6)},scale=${tileWidth}:-1,tile=${columns}x${rows}`;
  return ['-y', '-i', input, '-vf', vf, '-frames:v', '1', out];
}

export function waveformArgs(input: string, out: string, kind: 'waveform' | 'spectrogram', width: number, height: number): string[] {
  const filter = kind === 'spectrogram'
    ? `showspectrumpic=s=${width}x${height}:legend=disabled`
    : `showwavespic=s=${width}x${height}:colors=#3b82f6`;
  return ['-y', '-i', input, '-lavfi', filter, '-frames:v', '1', out];
}

export function gifArgs(input: string, out: string, opts: { fps: number; width: number; from?: string; to?: string }): string[] {
  const pre: string[] = ['-y'];
  if (opts.from) pre.push('-ss', opts.from);
  if (opts.to) pre.push('-to', opts.to);
  pre.push('-i', input);
  const filter = `fps=${opts.fps},scale=${opts.width}:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse`;
  return [...pre, '-filter_complex', filter, out];
}

export function trimArgs(
  input: string,
  out: string,
  from: string,
  to: string | undefined,
  duration: string | undefined,
  reencode: boolean,
  opts?: { videoCodec?: string; audioCodec?: string },
): string[] {
  const vcodec: Record<string, string> = { h264: 'libx264', hevc: 'libx265', vp9: 'libvpx-vp9', av1: 'libsvtav1' };
  const acodec: Record<string, string> = { mp3: 'libmp3lame', aac: 'aac', wav: 'pcm_s16le', flac: 'flac', copy: 'copy' };
  const args = ['-y', '-ss', from, '-i', input];
  if (to) args.push('-to', to);
  else if (duration) args.push('-t', duration);
  if (reencode) {
    const vc = opts?.videoCodec ?? 'h264';
    const ac = opts?.audioCodec ?? 'aac';
    args.push('-c:v', vcodec[vc] ?? 'libx264', '-pix_fmt', 'yuv420p');
    if (ac === 'none') args.push('-an');
    else args.push('-c:a', acodec[ac] ?? 'aac');
  } else {
    args.push('-c', 'copy');
  }
  args.push(out);
  return args;
}

export function audioArgs(input: string, out: string, format: string, bitrate?: string): string[] {
  const codec: Record<string, string> = { mp3: 'libmp3lame', aac: 'aac', wav: 'pcm_s16le', flac: 'flac' };
  const args = ['-y', '-i', input, '-vn', '-c:a', codec[format] ?? 'libmp3lame'];
  if (bitrate && /^\d+k?$/.test(bitrate)) args.push('-b:a', bitrate.endsWith('k') ? bitrate : `${bitrate}k`);
  args.push(out);
  return args;
}

const HW_CODECS = new Set(['h264_videotoolbox', 'hevc_videotoolbox', 'prores_videotoolbox']);

export function convertArgs(
  input: string,
  out: string,
  opts: { videoCodec?: string; audioCodec?: string; scale?: string; fps?: number; crf?: number; bitrate?: string },
): string[] {
  const vcodec: Record<string, string> = {
    h264: 'libx264', hevc: 'libx265', vp9: 'libvpx-vp9', av1: 'libsvtav1', copy: 'copy',
    h264_videotoolbox: 'h264_videotoolbox',
    hevc_videotoolbox: 'hevc_videotoolbox',
    prores_videotoolbox: 'prores_videotoolbox',
  };
  const acodec: Record<string, string> = { aac: 'aac', mp3: 'libmp3lame', copy: 'copy', none: '' };
  const vc = opts.videoCodec ?? 'h264';
  const isHw = HW_CODECS.has(vc);
  const isCopy = vc === 'copy';
  const args = ['-y', '-i', input];
  const filters: string[] = [];
  if (opts.scale && /^\d+x-?\d+$/.test(opts.scale)) filters.push(`scale=${opts.scale.replace('x', ':')}`);
  if (opts.fps) filters.push(`fps=${opts.fps}`);
  if (filters.length) args.push('-vf', filters.join(','));
  args.push('-c:v', vcodec[vc] ?? 'libx264');
  if (!isCopy && !isHw) {
    args.push('-pix_fmt', 'yuv420p');
    args.push('-crf', String(clampInt(opts.crf, 0, 51, 23)));
  } else if (isHw && opts.bitrate) {
    args.push('-b:v', opts.bitrate);
  }
  const ac = opts.audioCodec ?? 'aac';
  if (ac === 'none') args.push('-an');
  else args.push('-c:a', acodec[ac] || 'aac');
  args.push(out);
  return args;
}

/** Build argv for concat mode. `listFile` is a pre-written ffmpeg concat list file. */
export function concatArgs(
  listFile: string,
  out: string,
  reencode: boolean,
  opts?: { videoCodec?: string; audioCodec?: string; crf?: number },
): string[] {
  const vcodec: Record<string, string> = { h264: 'libx264', hevc: 'libx265', vp9: 'libvpx-vp9', av1: 'libsvtav1' };
  const acodec: Record<string, string> = { mp3: 'libmp3lame', aac: 'aac', copy: 'copy' };
  const args = ['-y', '-f', 'concat', '-safe', '0', '-i', listFile];
  if (reencode) {
    const vc = opts?.videoCodec ?? 'h264';
    const ac = opts?.audioCodec ?? 'aac';
    args.push('-c:v', vcodec[vc] ?? 'libx264', '-pix_fmt', 'yuv420p');
    if (opts?.crf !== undefined) args.push('-crf', String(clampInt(opts.crf, 0, 51, 23)));
    if (ac === 'none') args.push('-an');
    else args.push('-c:a', acodec[ac] ?? 'aac');
  } else {
    args.push('-c', 'copy');
  }
  args.push(out);
  return args;
}

// ---------------------------------------------------------------------------
// Core executor (Pi-agnostic) — validate, run, return MediaResult
// ---------------------------------------------------------------------------

function tempPng(cwd: string): string {
  const dir = path.join(extensionWorkspaceRoot(cwd), 'tmp', 'media');
  fs.mkdirSync(dir, { recursive: true });
  return path.join(dir, `frame-${process.pid}-${globalThis.performance.now().toString(36).replace('.', '')}.png`);
}

export function readPngForModel(file: string): { base64: string; bytes: number } {
  const size = fs.statSync(file).size;
  if (size > MAX_MODEL_MEDIA_IMAGE_BYTES) {
    throw new Error('media: generated preview exceeds the 4MB model/UI limit. Reduce dimensions, tile count, or duration.');
  }
  const buf = fs.readFileSync(file);
  return { base64: buf.toString('base64'), bytes: buf.length };
}

export async function runMediaQuery(
  query: Record<string, unknown>,
  cwd: string,
  signal?: AbortSignal,
  onProgress?: (fields: Record<string, string>) => void,
): Promise<MediaResult> {
  const det = detectFfmpeg();
  if (!det.ok) throw new Error(det.reason ?? 'ffmpeg unavailable');

  const mode = query['mode'] as Mode;
  if (!MODES.includes(mode)) throw new Error(`media: \`mode\` must be one of ${MODES.join(', ')}`);
  const timeoutMs = clampInt(query['timeoutSec'], 1, 1800, 120)! * 1000;
  const overwrite = query['overwrite'] === true;

  // --- Concat mode: multiple inputs — must branch before single-input resolution ---
  if (mode === 'concat') {
    const rawSources = Array.isArray(query['sources']) ? (query['sources'] as unknown[]) : [];
    if (rawSources.length < 2) throw new Error('media concat: `sources` must have at least 2 entries.');
    const sources = rawSources.map((s) => resolveInput(s, cwd));
    const out = resolveOutput(query['output'], cwd, overwrite);
    const reencode = query['reencode'] === true;
    const listContent = sources.map((s) => `file '${s}'`).join('\n') + '\n';
    const listFile = path.join(extensionWorkspaceRoot(cwd), 'tmp', 'media', `concat-${process.pid}-${Date.now()}.txt`);
    fs.mkdirSync(path.dirname(listFile), { recursive: true });
    fs.writeFileSync(listFile, listContent, 'utf8');
    const args = concatArgs(listFile, out, reencode, {
      videoCodec: query['videoCodec'] as string | undefined,
      audioCodec: query['audioCodec'] as string | undefined,
      crf: clampInt(query['crf'], 0, 51),
    });
    try {
      await runFfmpeg(args, { cwd, signal, timeoutMs, onProgress });
    } finally {
      try { fs.rmSync(listFile, { force: true }); } catch { /* best effort */ }
    }
    if (!fs.existsSync(out)) throw new Error('media concat: ffmpeg reported success but wrote no output file.');
    const outProbe = summarizeProbe(await runFfprobeJson(out, { cwd, signal, timeoutMs }).catch(() => ({})));
    const bytes = fs.statSync(out).size;
    return {
      ok: true, mode, savedPath: out, bytes, probe: outProbe,
      message: `wrote ${path.basename(out)} — ${describeProbe(outProbe)} [${formatBytes(bytes)}] (${sources.length} sources)`,
    };
  }

  const input = resolveInput(query['input'], cwd);

  // Probe is used both as its own mode and as a summary for produce-modes.
  const probeJson = await runFfprobeJson(input, { cwd, signal, timeoutMs });
  const probe = summarizeProbe(probeJson);

  if (mode === 'probe') {
    return { ok: true, mode, message: `probe ${path.basename(input)} — ${describeProbe(probe)}`, probe };
  }

  // --- Image modes: render to a temp PNG, read it back inline ---
  if (IMAGE_MODES.has(mode)) {
    const tmp = tempPng(cwd);
    try {
      let args: string[];
      if (mode === 'frame') {
        args = frameArgs(input, tmp, requireTimestamp(query['at'] ?? '0', 'at'), clampInt(query['width'], 1, MAX_DIMENSION));
      } else if (mode === 'contactSheet') {
        const count = clampInt(query['count'], 1, MAX_TILES, 9)!;
        const columns = clampInt(query['columns'], 1, MAX_TILES, Math.ceil(Math.sqrt(count)))!;
        const tileWidth = clampInt(query['width'], 16, MAX_DIMENSION, 320)!;
        args = contactSheetArgs(input, tmp, probe.durationSec ?? 0, count, columns, tileWidth);
      } else {
        const kind = query['kind'] === 'spectrogram' ? 'spectrogram' : 'waveform';
        args = waveformArgs(input, tmp, kind, clampInt(query['width'], 16, MAX_DIMENSION, 640)!, clampInt(query['height'], 16, MAX_DIMENSION, kind === 'spectrogram' ? 320 : 160)!);
      }
      await runFfmpeg(args, { cwd, signal, timeoutMs });
      if (!fs.existsSync(tmp)) throw new Error('media: ffmpeg produced no image (check timestamp is within the clip).');
      const { base64, bytes } = readPngForModel(tmp);

      // Optionally persist to a user-requested path.
      let savedPath: string | undefined;
      if (typeof query['output'] === 'string' && query['output'].trim()) {
        savedPath = resolveOutput(query['output'], cwd, overwrite);
        fs.copyFileSync(tmp, savedPath);
      }
      const savedNote = savedPath ? ` → ${path.basename(savedPath)}` : '';
      return {
        ok: true, mode, base64, bytes, mimeType: 'image/png', savedPath, probe,
        message: `${mode} ${path.basename(input)} [png, ${formatBytes(bytes)}]${savedNote}`,
      };
    } finally {
      try { fs.rmSync(tmp, { force: true }); } catch { /* best effort */ }
    }
  }

  // --- Produce modes: write a path-guarded output file ---
  const out = resolveOutput(query['output'], cwd, overwrite);
  let args: string[];
  if (mode === 'gif') {
    args = gifArgs(input, out, {
      fps: clampInt(query['fps'], 1, 50, 12)!,
      width: clampInt(query['width'], 16, MAX_DIMENSION, 480)!,
      from: query['from'] ? requireTimestamp(query['from'], 'from') : undefined,
      to: query['to'] ? requireTimestamp(query['to'], 'to') : undefined,
    });
  } else if (mode === 'trim') {
    args = trimArgs(
      input, out,
      requireTimestamp(query['from'], 'from'),
      query['to'] ? requireTimestamp(query['to'], 'to') : undefined,
      query['duration'] ? requireTimestamp(query['duration'], 'duration') : undefined,
      query['reencode'] === true,
      {
        videoCodec: query['videoCodec'] as string | undefined,
        audioCodec: query['audioCodec'] as string | undefined,
      },
    );
  } else if (mode === 'audio') {
    const format = ['mp3', 'aac', 'wav', 'flac'].includes(query['format'] as string) ? (query['format'] as string) : 'mp3';
    args = audioArgs(input, out, format, typeof query['bitrate'] === 'string' ? query['bitrate'] : undefined);
  } else {
    args = convertArgs(input, out, {
      videoCodec: query['videoCodec'] as string | undefined,
      audioCodec: query['audioCodec'] as string | undefined,
      scale: query['scale'] as string | undefined,
      fps: clampInt(query['fps'], 1, 240),
      crf: clampInt(query['crf'], 0, 51),
      bitrate: query['bitrate'] as string | undefined,
    });
  }
  await runFfmpeg(args, { cwd, signal, timeoutMs, onProgress });
  if (!fs.existsSync(out)) throw new Error('media: ffmpeg reported success but wrote no output file.');
  const outProbe = summarizeProbe(await runFfprobeJson(out, { cwd, signal, timeoutMs }).catch(() => ({})));
  const bytes = fs.statSync(out).size;
  return {
    ok: true, mode, savedPath: out, bytes, probe: outProbe,
    message: `wrote ${path.basename(out)} — ${describeProbe(outProbe)} [${formatBytes(bytes)}]`,
  };
}

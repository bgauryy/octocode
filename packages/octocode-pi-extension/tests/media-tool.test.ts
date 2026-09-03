/**
 * Tests for media-tool.ts argv builders.
 * Pure functions — no ffmpeg binary required.
 *
 * Covers:
 *  - TODO 5: trimArgs respects videoCodec / audioCodec on reencode
 *  - TODO 1: convertArgs supports h264_videotoolbox / hevc_videotoolbox
 *  - TODO 2: concatArgs builds correct -f concat argv
 *  - TODO 3: detectFfmpeg falls back to ffmpeg-static when PATH empty
 */
import assert from 'node:assert/strict';
import { test, describe } from 'vitest';
import {
  trimArgs,
  convertArgs,
  concatArgs,
  isValidTimestamp,
  readPngForModel,
} from '../src/tools/media-tool.js';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  detectFfmpeg,
  resetFfmpegDetectionForTests,
} from '../src/tools/ffmpeg-runtime.js';

// ---------------------------------------------------------------------------
// isValidTimestamp (existing, sanity)
// ---------------------------------------------------------------------------
describe('isValidTimestamp', () => {
  test('accepts HH:MM:SS', () => assert.ok(isValidTimestamp('1:23:45')));
  test('accepts MM:SS', () => assert.ok(isValidTimestamp('0:05')));
  test('accepts decimal seconds', () => assert.ok(isValidTimestamp('90.5')));
  test('rejects empty string', () => assert.ok(!isValidTimestamp('')));
  test('rejects letters', () => assert.ok(!isValidTimestamp('abc')));
});

test('readPngForModel rejects oversized previews before base64 allocation', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'media-preview-cap-'));
  const file = path.join(dir, 'huge.png');
  try {
    fs.writeFileSync(file, Buffer.alloc(4 * 1024 * 1024 + 1));
    assert.throws(() => readPngForModel(file), /4MB/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

// ---------------------------------------------------------------------------
// TODO 5 — trimArgs: reencode respects videoCodec / audioCodec
// ---------------------------------------------------------------------------
describe('trimArgs — reencode codec passthrough', () => {
  const IN = '/input.mp4';
  const OUT = '/out.mp4';

  test('stream copy (default) uses -c copy', () => {
    const args = trimArgs(IN, OUT, '0:01', '0:05', undefined, false);
    assert.ok(args.includes('copy'), 'should have copy codec');
    assert.ok(!args.includes('libx264'), 'should not include libx264');
  });

  test('reencode with no codec defaults to libx264 + aac', () => {
    const args = trimArgs(IN, OUT, '0:01', '0:05', undefined, true);
    assert.ok(args.includes('libx264'), 'default video codec is libx264');
    assert.ok(args.includes('aac'), 'default audio codec is aac');
  });

  test('reencode with videoCodec:hevc uses libx265', () => {
    const args = trimArgs(IN, OUT, '0:01', '0:05', undefined, true, { videoCodec: 'hevc' });
    assert.ok(args.includes('libx265'), 'hevc maps to libx265');
    assert.ok(!args.includes('libx264'), 'should not include libx264 when hevc');
  });

  test('reencode with videoCodec:vp9 uses libvpx-vp9', () => {
    const args = trimArgs(IN, OUT, '0:01', '0:05', undefined, true, { videoCodec: 'vp9' });
    assert.ok(args.includes('libvpx-vp9'), 'vp9 maps to libvpx-vp9');
  });

  test('reencode with audioCodec:mp3 uses libmp3lame', () => {
    const args = trimArgs(IN, OUT, '0:01', '0:05', undefined, true, { audioCodec: 'mp3' });
    assert.ok(args.includes('libmp3lame'), 'mp3 maps to libmp3lame');
    assert.ok(!args.includes('aac'), 'should not include aac when mp3');
  });

  test('reencode with audioCodec:none adds -an', () => {
    const args = trimArgs(IN, OUT, '0:01', '0:05', undefined, true, { audioCodec: 'none' });
    assert.ok(args.includes('-an'), 'none maps to -an');
    assert.ok(!args.includes('aac'), 'no aac when audio:none');
  });

  test('reencode with both codecs overrides both', () => {
    const args = trimArgs(IN, OUT, '0:01', '0:05', undefined, true, {
      videoCodec: 'hevc',
      audioCodec: 'copy',
    });
    assert.ok(args.includes('libx265'), 'video: hevc');
    const caIdx = args.indexOf('-c:a');
    assert.ok(caIdx !== -1 && args[caIdx + 1] === 'copy', 'audio: copy');
  });

  test('unknown videoCodec falls back to libx264', () => {
    const args = trimArgs(IN, OUT, '0:01', '0:05', undefined, true, { videoCodec: 'unknown' });
    assert.ok(args.includes('libx264'), 'unknown codec falls back to libx264');
  });

  test('duration is used when to is undefined', () => {
    const args = trimArgs(IN, OUT, '0:01', undefined, '4', true);
    assert.ok(args.includes('-t'), 'uses -t for duration');
    assert.ok(args.includes('4'), 'includes duration value');
  });
});

// ---------------------------------------------------------------------------
// TODO 1 — convertArgs: VideoToolbox hw codecs
// ---------------------------------------------------------------------------
describe('convertArgs — VideoToolbox hw codecs', () => {
  const IN = '/input.mp4';
  const OUT = '/out.mp4';

  test('h264_videotoolbox is passed through to -c:v', () => {
    const args = convertArgs(IN, OUT, { videoCodec: 'h264_videotoolbox' });
    assert.ok(args.includes('h264_videotoolbox'), '-c:v h264_videotoolbox');
  });

  test('hevc_videotoolbox is passed through to -c:v', () => {
    const args = convertArgs(IN, OUT, { videoCodec: 'hevc_videotoolbox' });
    assert.ok(args.includes('hevc_videotoolbox'), '-c:v hevc_videotoolbox');
  });

  test('h264_videotoolbox does NOT emit -pix_fmt yuv420p', () => {
    const args = convertArgs(IN, OUT, { videoCodec: 'h264_videotoolbox' });
    assert.ok(!args.includes('yuv420p'), 'VT does not need pix_fmt');
  });

  test('h264_videotoolbox does NOT emit -crf', () => {
    const args = convertArgs(IN, OUT, { videoCodec: 'h264_videotoolbox', crf: 23 });
    assert.ok(!args.includes('-crf'), 'VT does not support CRF');
  });

  test('h264_videotoolbox with bitrate emits -b:v', () => {
    const args = convertArgs(IN, OUT, { videoCodec: 'h264_videotoolbox', bitrate: '4M' });
    const bvIdx = args.indexOf('-b:v');
    assert.ok(bvIdx !== -1 && args[bvIdx + 1] === '4M', '-b:v 4M present');
  });

  test('software codec (h264) still emits pix_fmt and crf', () => {
    const args = convertArgs(IN, OUT, { videoCodec: 'h264', crf: 28 });
    assert.ok(args.includes('yuv420p'), 'software codec needs pix_fmt');
    assert.ok(args.includes('-crf'), 'software codec supports CRF');
    assert.ok(args.includes('28'), 'crf value present');
  });

  test('software codec without crf uses default 23', () => {
    const args = convertArgs(IN, OUT, { videoCodec: 'h264' });
    assert.ok(args.includes('23'), 'default crf is 23');
  });

  test('copy codec skips pix_fmt and crf', () => {
    const args = convertArgs(IN, OUT, { videoCodec: 'copy' });
    assert.ok(!args.includes('yuv420p'), 'copy skips pix_fmt');
    assert.ok(!args.includes('-crf'), 'copy skips crf');
  });
});

// ---------------------------------------------------------------------------
// TODO 2 — concatArgs: new concat type
// ---------------------------------------------------------------------------
describe('concatArgs — stream copy and reencode', () => {
  const LIST = '/tmp/list.txt';
  const OUT = '/out.mp4';

  test('stream copy (default) uses -c copy', () => {
    const args = concatArgs(LIST, OUT, false);
    assert.ok(args.includes('-f'), '-f present');
    assert.ok(args.includes('concat'), '-f concat');
    assert.ok(args.includes('-safe'), '-safe present');
    assert.ok(args.includes('0'), '-safe 0');
    assert.ok(args.includes('-c'), '-c present');
    assert.ok(args.includes('copy'), '-c copy');
    assert.ok(args.includes(LIST), 'list file is -i arg');
    assert.ok(args.includes(OUT), 'output path present');
  });

  test('reencode uses libx264 + aac by default', () => {
    const args = concatArgs(LIST, OUT, true);
    assert.ok(args.includes('libx264'), 'default reencode video: libx264');
    assert.ok(args.includes('aac'), 'default reencode audio: aac');
    assert.ok(!args.includes('-c'), 'no -c copy when reencoding');
  });

  test('reencode with hevc uses libx265', () => {
    const args = concatArgs(LIST, OUT, true, { videoCodec: 'hevc' });
    assert.ok(args.includes('libx265'), 'hevc: libx265');
  });

  test('reencode with audioCodec:none adds -an', () => {
    const args = concatArgs(LIST, OUT, true, { audioCodec: 'none' });
    assert.ok(args.includes('-an'), '-an for no audio');
  });

  test('always emits -y (overwrite) and -i <list>', () => {
    const args = concatArgs(LIST, OUT, false);
    assert.ok(args[0] === '-y', 'first arg is -y');
    const iIdx = args.indexOf('-i');
    assert.ok(iIdx !== -1 && args[iIdx + 1] === LIST, '-i <list>');
  });
});

// ---------------------------------------------------------------------------
// detectFfmpeg cache contract — availability is injected, never host-detected
// ---------------------------------------------------------------------------
describe('detectFfmpeg — mocked availability', () => {
  test('detectFfmpeg caches an injected unavailable result', () => {
    const unavailable = { ok: false as const, reason: 'mock ffmpeg unavailable' };
    resetFfmpegDetectionForTests(unavailable);
    const r1 = detectFfmpeg();
    const r2 = detectFfmpeg();
    assert.deepEqual(r1, r2, 'same result on repeated calls');
    assert.deepEqual(r1, unavailable);
    resetFfmpegDetectionForTests(undefined);
  });

  test('resetFfmpegDetectionForTests clears cache and accepts override', () => {
    const fakeAvail = { ok: true as const, ffmpeg: '/fake/ffmpeg', ffprobe: '/fake/ffprobe' };
    resetFfmpegDetectionForTests(fakeAvail);
    const result = detectFfmpeg();
    assert.deepEqual(result, fakeAvail, 'override is returned');
    resetFfmpegDetectionForTests(undefined);
  });
});

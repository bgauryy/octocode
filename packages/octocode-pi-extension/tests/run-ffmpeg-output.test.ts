import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import { persistFfmpegStdout } from '../src/tools/run-ffmpeg-tool.js';

test('captured ffmpeg stdout is always artifact-backed, even without Pi session context', () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'ffmpeg-artifact-'));
  const prior = process.env['OCTOCODE_HOME'];
  process.env['OCTOCODE_HOME'] = cwd;
  try {
    const bytes = Buffer.from([0, 1, 2, 3, 255]);
    const artifact = persistFfmpegStdout(bytes, cwd, undefined, 'call/unsafe');
    assert.equal(fs.readFileSync(artifact).equals(bytes), true);
    assert.match(artifact, /call-unsafe-stdout\.bin$/);
  } finally {
    if (prior === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = prior;
    fs.rmSync(cwd, { recursive: true, force: true });
  }
});

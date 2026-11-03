import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { createLiveProject } from './grammar-live-projects.mjs';
import { GRAMMAR_FIXTURES } from './grammar-fixtures.mjs';
import { grammarAnchors } from './grammar-lsp.mjs';

const require = createRequire(import.meta.url);
const ts = require('typescript');

test('real JavaScript projects retain strict checking and exact anchors in every module format', async () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-live-js-test-'));
  const original = GRAMMAR_FIXTURES.JavaScript.src;
  try {
    for (const ext of ['js', 'jsx', 'mjs', 'cjs']) {
      const project = await createLiveProject({ tmp, capability: { language: 'JavaScript' }, ext,
        fixture: GRAMMAR_FIXTURES.JavaScript, env: process.env, report: { setup: [] } });
      const configPath = path.join(project.cwd, 'tsconfig.json');
      const config = ts.readConfigFile(configPath, file => ts.sys.readFile(file));
      assert.equal(config.error, undefined);
      const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, project.cwd);
      for (const flag of ['allowJs', 'checkJs', 'strict', 'noImplicitAny', 'noEmit']) assert.equal(parsed.options[flag], true, `${ext}:${flag}`);
      const program = ts.createProgram(parsed.fileNames, parsed.options);
      const diagnostics = ts.getPreEmitDiagnostics(program);
      assert.deepEqual(diagnostics.map(diagnostic => ({ code: diagnostic.code, message: ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n') })), [], ext);
      const anchors = grammarAnchors('JavaScript', project.fixture);
      const before = grammarAnchors('JavaScript', GRAMMAR_FIXTURES.JavaScript);
      assert.equal(anchors.locations.length, before.locations.length, 'JSDoc adds no helper reference');
      assert.deepEqual(anchors.position, before.position);
      assert.equal(anchors.target.start.line, before.target.start.line);
      assert.ok(anchors.target.start.character > before.target.start.character);
      if (ext === 'mjs') {
        assert.match(project.fixture.src, /export \{ Widget, helper, run \};/u);
        assert.doesNotMatch(project.fixture.src, /module\.exports/u);
        const run = spawnSync(process.execPath, [project.file], { encoding: 'utf8' });
        assert.equal(run.status, 0, run.stderr);
      } else assert.match(project.fixture.src, /module\.exports/u);
    }
    assert.equal(GRAMMAR_FIXTURES.JavaScript.src, original, 'shared fixture is immutable');
  } finally { fs.rmSync(tmp, { recursive: true, force: true }); }
});

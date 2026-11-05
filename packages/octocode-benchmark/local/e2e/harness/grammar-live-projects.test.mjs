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
      const project = await createLiveProject({
        tmp,
        capability: { language: 'JavaScript' },
        ext,
        fixture: GRAMMAR_FIXTURES.JavaScript,
        env: process.env,
        report: { setup: [] },
      });
      const configPath = path.join(project.cwd, 'tsconfig.json');
      const config = ts.readConfigFile(configPath, file =>
        ts.sys.readFile(file)
      );
      assert.equal(config.error, undefined);
      const parsed = ts.parseJsonConfigFileContent(
        config.config,
        ts.sys,
        project.cwd
      );
      for (const flag of [
        'allowJs',
        'checkJs',
        'strict',
        'noImplicitAny',
        'noEmit',
      ])
        assert.equal(parsed.options[flag], true, `${ext}:${flag}`);
      const program = ts.createProgram(parsed.fileNames, parsed.options);
      const diagnostics = ts.getPreEmitDiagnostics(program);
      assert.deepEqual(
        diagnostics.map(diagnostic => ({
          code: diagnostic.code,
          message: ts.flattenDiagnosticMessageText(
            diagnostic.messageText,
            '\n'
          ),
        })),
        [],
        ext
      );
      const anchors = grammarAnchors('JavaScript', project.fixture);
      const before = grammarAnchors('JavaScript', GRAMMAR_FIXTURES.JavaScript);
      assert.equal(
        anchors.locations.length,
        before.locations.length,
        'JSDoc adds no helper reference'
      );
      assert.deepEqual(anchors.position, before.position);
      assert.equal(anchors.target.start.line, before.target.start.line);
      assert.ok(anchors.target.start.character > before.target.start.character);
      if (ext === 'mjs') {
        assert.match(project.fixture.src, /export \{ Widget, helper, run \};/u);
        assert.doesNotMatch(project.fixture.src, /module\.exports/u);
        const run = spawnSync(process.execPath, [project.file], {
          encoding: 'utf8',
        });
        assert.equal(run.status, 0, run.stderr);
      } else assert.match(project.fixture.src, /module\.exports/u);
    }
    assert.equal(
      GRAMMAR_FIXTURES.JavaScript.src,
      original,
      'shared fixture is immutable'
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('a clangd project states its claim, and a database project holds a real entry and no flags file', async () => {
  const tmp = fs.realpathSync(
    fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-live-c-test-'))
  );
  try {
    const make = (language, ext, compileDatabase) =>
      createLiveProject({
        tmp,
        capability: { language },
        ext,
        fixture: GRAMMAR_FIXTURES[language],
        env: process.env,
        report: { setup: [] },
        compileDatabase,
      });
    assert.deepEqual((await make('C', 'c')).coverage, {
      unindexed: { reason: 'noCompileDatabase', configuration: 'notFound' },
    });
    const flags = await make('CUDA', 'cu');
    assert.deepEqual(flags.coverage, {
      unindexed: { reason: 'noCompileDatabase', configuration: 'found' },
    });
    assert.ok(fs.existsSync(path.join(flags.cwd, 'compile_flags.txt')));
    for (const [language, ext, flag] of [
      ['C', 'c', '-xc'],
      ['C++', 'cpp', '-xc++'],
      ['CUDA', 'cu', '-xcuda'],
    ]) {
      const project = await make(language, ext, true);
      assert.deepEqual(project.coverage, {
        database: path.join(project.cwd, 'compile_commands.json'),
      });
      assert.equal(
        fs.existsSync(path.join(project.cwd, 'compile_flags.txt')),
        false
      );
      const [entry, ...rest] = JSON.parse(
        fs.readFileSync(project.coverage.database, 'utf8')
      );
      assert.deepEqual(rest, []);
      assert.equal(entry.directory, project.cwd);
      assert.equal(entry.file, project.file);
      assert.ok(fs.existsSync(entry.file));
      assert.ok(
        entry.arguments.includes(flag) &&
          entry.arguments.at(-1) === project.file
      );
    }
    assert.deepEqual((await make('Python', 'py')).coverage, {});
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

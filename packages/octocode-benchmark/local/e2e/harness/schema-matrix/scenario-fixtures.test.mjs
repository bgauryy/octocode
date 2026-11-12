import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { CODE_SCENARIOS } from './scenarios-codes.mjs';
import { LEAD_SCENARIOS } from './scenarios-leads.mjs';
import { REASON_SCENARIOS } from './scenarios-reasons.mjs';
import { contract } from '../contract.mjs';

const scene = id => [...CODE_SCENARIOS, ...LEAD_SCENARIOS, ...REASON_SCENARIOS].find(row => row.id === id);

test('checkout failures intercept HEAD verification after GitRunner options', () => {
  for (const [id, head] of [['code.commitMismatch', 'f'.repeat(40)], ['code.gitFailed', 'not-a-sha']]) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'matrix-git-'));
    try {
      scene(id).setup({ dir });
      const git = path.join(dir, 'bin/git');
      for (const prefix of [[], ['-c', 'core.hooksPath=/dev/null'], ['-c', 'core.hooksPath=/dev/null', '-c', 'advice.detachedHead=false']]) {
        const result = spawnSync(git, [...prefix, '-C', dir, 'rev-parse', '--verify', 'HEAD^{commit}'], { encoding: 'utf8' });
        assert.equal(result.status, 0, result.stderr);
        assert.equal(result.stdout, `${head}\n`);
      }
      const ordinary = spawnSync(git, ['--version'], { encoding: 'utf8' });
      assert.equal(ordinary.status, 0, ordinary.stderr);
      assert.match(ordinary.stdout, /^git version /);
    } finally { fs.rmSync(dir, { recursive: true, force: true }); }
  }
});

test('declaration clipping starts from the real use of the symbol', () => {
  const row = scene('lsp.declarationClipped');
  const lines = row.files['a.ts'].split('\n');
  assert.equal(lines[row.query.line - 1], 'f();');
  const answer = row.lsp.script('file:///fixture').answers[0].result[0];
  assert.equal(row.query.contextLines.after, answer.targetRange.end.line);
  assert.equal(row.query.contextLines.before, 0);
  assert.ok(lines.slice(0, row.query.contextLines.after + 1).join('\n').length > contract.limits.defaultResponseChars);
  assert.deepEqual(row.reasons, ['declarationClipped']);
  assert.deepEqual(row.channels, ['readDeclaration']);
});

test('missing-version repair executes the offered related read unchanged and rejects wrong identity', async () => {
  const row = scene('lead.repair.search');
  assert.deepEqual(row.codes, ['notFound']);
  const input = { queries: [{ ...row.query, version: '1.2.3' }] };
  const lead = { kind: 'related', tool: 'artifactSearch', input };
  const invoke = async replacement => {
    let calls = 0;
    const result = await row.run({ walk: async (tool, request) => {
      assert.equal(tool, 'artifactSearch');
      if (calls++ === 0) return { failures: [], pages: [{ json: { results: [{ data: { errorCode: 'notFound', related: [replacement] } }] } }] };
      assert.equal(request, input, 'execute the emitted call unchanged');
      return { failures: [], pages: [{ json: { results: [{ data: { artifacts: [{ version: '1.2.3' }] } }] } }] };
    } });
    return { result, calls };
  };
  assert.deepEqual(await invoke(lead), { result: null, calls: 2 });
  assert.equal((await invoke({ ...lead, kind: 'continue' })).calls, 1);
  const wrong = await invoke({ ...lead, input: { queries: [{ ...input.queries[0], packageName: 'other' }] } });
  assert.match(wrong.result, /package identity/);
  assert.equal(wrong.calls, 1);
});

test('unknown-only recovery has both judged and unknown source chunks', () => {
  const local = scene('lead.unfiltered.local');
  const lines = local.files['a.txt'].split('\n');
  for (const id of ['lead.unfiltered.local', 'lead.unfiltered.github']) {
    const row = scene(id);
    const judge = content => row.provider({ state: { content }, questions: { q: { type: 'score', criteria: ['low', 'mid', 'high'] } } }).body.answers.q;
    assert.equal(judge(lines[0]).confidence, 1);
    assert.equal(judge(lines.at(-1)).confidence, 0.4);
    assert.notDeepEqual(judge(lines[0]), judge(lines.at(-1)));
  }
});

test('node-limit fixture uses unique source anchors and reanchors every node', () => {
  const row = scene('lsp.hierarchyNodeLimit');
  const source = row.files['src/lib.rs'].split('\n');
  const replies = row.lsp.script('file:///fixture').answers;
  const prepared = replies.filter(answer => answer.method === 'textDocument/prepareCallHierarchy');
  const identities = new Set();
  for (const answer of prepared) {
    const node = answer.result[0];
    const { start, end } = node.selectionRange;
    assert.equal(source[start.line].slice(start.character, end.character), node.name);
    assert.equal(answer.word, node.name);
    identities.add(`${node.uri}:${JSON.stringify(node.selectionRange)}`);
  }
  assert.equal(identities.size, prepared.length);
  const root = fileURLToPath(new URL('../../../../../../', import.meta.url));
  const owner = fs.readFileSync(path.join(root, 'packages/octocode-native/crates/runtime/src/tools/lsp_search/walk.rs'), 'utf8');
  const cap = Number(/MAX_HIERARCHY_NODES: usize = (\d+)/.exec(owner)[1]);
  assert.ok(identities.size > cap, `${identities.size} nodes must exceed the owning cap ${cap}`);
});

test('parse recovery includes the owner fixture whose error hides the anchor', () => {
  const root = fileURLToPath(new URL('../../../../../../', import.meta.url));
  const owner = fs.readFileSync(path.join(root, 'packages/octocode-native/crates/runtime/src/tools/ast_search/match_tests/grammars.rs'), 'utf8');
  const garbled = JSON.parse(/const GARBLED_CPP: &str = (".*");/.exec(owner)[1]);
  assert.ok(Object.values(scene('reason.parseRecovered').files).includes(garbled));
});

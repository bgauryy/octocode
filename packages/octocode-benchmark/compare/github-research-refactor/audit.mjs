// Read-only replay of frozen artifacts; this reporting utility is not part of the solver harness.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { grade } from './grading.mjs';
import { stableCatalog, toolFailed } from '../graph-research-v1/run.mjs';
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(process.argv[2] ?? '.octocode/benchmarks/github-research-refactor');
const read = path => JSON.parse(readFileSync(path, 'utf8'));
const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const manifest = read(root + '/manifest.json'), preflight = read(root + '/audit-preflight.json');
assert.equal(hash(root + '/manifest.json'), preflight.manifestSha256);
assert.equal(hash(root + '/sources.json'), preflight.sourcesSha256);
for (const [file, sha] of Object.entries(manifest.harness)) assert.equal(hash(resolve(here, file)), sha);
let trials = 0, catalogs = 0, hostTokens = { baseline: 0, candidate: 0 }, callsCount = { baseline: 0, candidate: 0 };
for (const arm of ['baseline', 'candidate']) {
  const subject = read(root + '/' + arm + '.json');
  assert.equal(hash(root + '/' + arm + '.json'), read(root + '/' + arm + '-freeze.json').sha256);
  for (const task of manifest.cases) {
    const path = `${root}/trials/${task.id}/${arm}`;
    const events = readFileSync(path + '/calls.jsonl', 'utf8').trim().split('\n').map(JSON.parse);
    const calls = events.filter(row => row.event === 'call'), result = read(path + '/result.json');
    assert(stableCatalog(events, subject.catalog));
    for (const catalog of events.filter(row => row.event === 'catalog')) { assert.equal(catalog.instructions, subject.instructions); catalogs++; }
    assert(grade(task, read(path + '/answer.json'), calls));
    assert(result.valid && result.correct);
    assert.equal(result.receipt.actualModel, manifest.model);
    assert.equal(result.receipt.actualModelProvider, 'openai');
    assert.equal(result.receipt.exitCode, 0);
    assert.equal(result.receipt.usage.length, 1);
    assert.equal(result.receipt.prohibitedToolEvents, 0);
    assert.equal(result.receipt.declinedApprovals, 0);
    assert(calls.every(call => call.admitted));
    assert.equal(result.errors, calls.filter(call => toolFailed(call.result)).length);
    assert.equal(result.hostTokens, result.receipt.usage.reduce((n, u) => n + u.input_tokens + u.output_tokens, 0));
    assert(!existsSync(path + '/codex-home/auth.json'));
    hostTokens[arm] += result.hostTokens; callsCount[arm] += calls.length; trials++;
  }
}
const report = read(root + '/report.json');
for (const arm of ['baseline', 'candidate']) { assert.equal(report.totals[arm].hostTokens, hostTokens[arm]); assert.equal(report.totals[arm].calls, callsCount[arm]); }
assert.equal(report.hostTokenReduction, 1 - hostTokens.candidate / hostTokens.baseline);
assert.equal(report.verdict, 'BENEFIT_NOT_DEMONSTRATED');
console.log(JSON.stringify({ trials, verifiedAnswers: trials, catalogObservations: catalogs, hostTokens, calls: callsCount, credentialCopiesRemaining: 0, frozenHashesUnchanged: true, verdict: report.verdict }, null, 2));

// Deterministic CLI replay: measures retrieval bytes/latency, never model tokens.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, mkdirSync, writeFileSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

const root = fileURLToPath(new URL('../', import.meta.url));
const target = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
const binary = process.env.COMMUNICATION_BINARY ?? join(root, 'skills/octocode-agents-communication/scripts/bin', target, 'octocode-agents-communication');
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? join(root, '../../.octocode/benchmarks/communication-context-discovery/retrieval'));
mkdirSync(output, { recursive: true });
const contract = {
  comparison: 'Existing paginated audit discovery versus scoped context lookup over identical stored notes',
  cases: 6, repetitions: 3, noteCount: 32, unrelatedAuditRows: 220,
  gates: { exactRelevantNames: true, repeatedNotes: 0, medianDiscoveryByteReduction: 0.75, candidateP95Ms: 250 },
  limits: 'Synthetic path/branch fixture. No provider inference, token-cost or coding-quality claim. All pages count, including empty pages.',
};
writeFileSync(join(output, 'contract.json'), JSON.stringify(contract, null, 2) + '\n');
const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'communication-context-benchmark-')));
const database = join(workspace, 'audit.sqlite');
const base = ['--workspace', workspace, '--database', database];
let db;
try {
  const run = (args, input = {}) => {
    const started = performance.now();
    const text = execFileSync(binary, [...args, JSON.stringify(input), ...base], { encoding: 'utf8', timeout: 10000, maxBuffer: 4 * 1024 * 1024 });
    return { value: JSON.parse(text), bytes: Buffer.byteLength(text), ms: performance.now() - started };
  };
  const owner = run(['join'], { name: 'author', vendor: 'generic' }).value.id;
  const reader = run(['join'], { name: 'late-reader', vendor: 'generic' }).value.id;
  const call = (command, input = {}, session = reader) => run([command, '--session', session], input);
  db = new DatabaseSync(database);
  const insert = db.prepare("INSERT INTO audit(session,kind,entityId,at,data) VALUES(?,'fixture',?,?,'{}')");
  db.exec('BEGIN');
  for (let i = 0; i < contract.unrelatedAuditRows; i++) insert.run(owner, String(i), Date.now());
  db.exec('COMMIT');
  const notes = [];
  for (let group = 0; group < 8; group++) for (const branch of ['main', 'feature']) for (let n = 0; n < 2; n++) {
    const name = `group-${group}-${branch}-${n}.md`;
    notes.push({ name, group, branch });
    call('share_document', { name, reasoning: `Measure discovery of group ${group} evidence without reading unrelated bodies`, content: `Evidence for ${name}.\n` + 'Only fetch relevant evidence.\n'.repeat(80), context: { summary: `Group ${group}: preserve ${branch} invariant ${n}; callers depend on it.`, path: `src/group-${group}`, branch } }, owner);
  }
  const observations = [];
  for (let repeat = 0; repeat < contract.repetitions; repeat++) for (let group = 0; group < contract.cases; group++) {
    const branch = group % 2 ? 'feature' : 'main';
    const expected = notes.filter(note => note.group === group && note.branch === branch).map(note => note.name).sort();
    const baseline = () => {
      let input = {}, bytes = 0, ms = 0, calls = 0; const found = [];
      do {
        const result = run(['entity', 'list', 'audit', '--session', reader], input);
        bytes += result.bytes; ms += result.ms; calls++;
        found.push(...result.value.items.filter(row => row.kind === 'document.created'));
        input = result.value.next ? { after: result.value.next } : null;
        assert.ok(calls < 20, 'Baseline must terminate');
      } while (input);
      assert.equal(found.length, notes.length);
      return { bytes, ms, calls };
    };
    const candidate = () => {
      let input = { path: `src/group-${group}/file.rs`, branch }, bytes = 0, ms = 0, calls = 0, cursor;
      const found = [];
      do {
        const result = call('context', input); bytes += result.bytes; ms += result.ms; calls++;
        found.push(...result.value.items.map(note => note.name)); cursor = result.value.cursor;
        input = result.value.next; assert.ok(calls < 20, 'Candidate must terminate');
      } while (input);
      assert.deepEqual(found.sort(), expected);
      assert.deepEqual(call('context', { path: `src/group-${group}/file.rs`, branch, after: cursor }).value.items, []);
      return { bytes, ms, calls, names: found };
    };
    const pair = (repeat + group) % 2 ? { candidate: candidate(), baseline: baseline() } : { baseline: baseline(), candidate: candidate() };
    observations.push({ repeat, group, branch, ...pair });
  }
  const percentile = (values, q) => values.toSorted((a, b) => a - b)[Math.ceil(values.length * q) - 1];
  const reduction = percentile(observations.map(x => 1 - x.candidate.bytes / x.baseline.bytes), 0.5);
  const candidateP95Ms = percentile(observations.map(x => x.candidate.ms), 0.95);
  const report = { contract, binarySha256: createHash('sha256').update(readFileSync(binary)).digest('hex'), observations,
    medianDiscoveryByteReduction: reduction, candidateP95Ms,
    baselineP95Ms: percentile(observations.map(x => x.baseline.ms), 0.95),
    passed: reduction >= contract.gates.medianDiscoveryByteReduction && candidateP95Ms <= contract.gates.candidateP95Ms };
  writeFileSync(join(output, 'result.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ output, ...report, observations: undefined }));
  assert.ok(report.passed, 'Retrieval gates failed; keep evidence and investigate');
} finally { db?.close(); rmSync(workspace, { recursive: true, force: true }); }

// Post-hoc calibration audit. Never overwrites the frozen campaign score.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { grade } from './run.mjs';

export function audit(expected, answer, calls) {
  if (answer?.items?.length !== 1 || answer.items[0].id !== expected.id) return false;
  const item = answer.items[0];
  const normalize = value => expected.id === 'materialization' ? value.replace(/^GraphAnalysis::/, '') : value.split(',').map(part => part.trim()).join(',');
  if (normalize(item.value) !== expected.value) return false;
  const normalized = { items: [{ ...item, value: expected.value }] };
  if (grade(expected, normalized, calls)) return true;
  // Response pagination can omit structured rows while preserving numbered
  // source in the actual model-visible text. Require the exact source line.
  return item.evidence.some(cite => resolve(cite.path) === expected.path && cite.line === expected.line && calls.some(call => {
    if (!call.admitted || call.name !== 'localFetch' || call.result?.isError) return false;
    const queries = call.args.queries ?? [call.args];
    if (!queries.every(query => query.path === expected.path)) return false;
    return call.result?.content?.some(block => block.type === 'text' && block.text.split('\n').some(line => {
      const match = line.match(/^(\d+): (.*)$/);
      return match && Number(match[1]) === expected.line && match[2].includes(expected.needle);
    }));
  }));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === '--selftest') {
    const expected = { id: 'materialization', value: 'Drift', needle: 'let mut graph_builder =', path: '/fixture.rs', line: 98 };
    const answer = { items: [{ id: expected.id, value: 'GraphAnalysis::Drift', evidence: [{ path: expected.path, line: 98 }] }] };
    const call = { admitted: true, name: 'localFetch', args: { path: expected.path }, result: { content: [{ type: 'text', text: '98: let mut graph_builder = ...' }] } };
    assert.equal(audit(expected, answer, [call]), true);
    assert.equal(audit(expected, answer, [{ ...call, name: 'clasify' }]), false);
    assert.equal(audit(expected, answer, [{ ...call, args: { path: '/other.rs' } }]), false);
    assert.equal(audit(expected, answer, [{ ...call, result: { content: [{ type: 'text', text: '99: let mut graph_builder = ...' }] } }]), false);
    assert.equal(audit(expected, { items: [{ ...answer.items[0], value: 'Dependencies' }] }, [call]), false);
    console.log('5 post-hoc audit checks passed.');
  } else {
    if (!process.argv[2]) throw new Error('Campaign directory required');
    const root = resolve(process.argv[2]);
    const read = path => JSON.parse(readFileSync(path, 'utf8'));
    const manifest = read(join(root, 'manifest.json'));
    const records = manifest.cases.flatMap(task => ['baseline', 'candidate'].map(arm => {
      const dir = join(root, 'trials', task.id, arm);
      const result = read(join(dir, 'result.json'));
      const answer = read(join(dir, 'answer.json'));
      const calls = readFileSync(join(dir, 'calls.jsonl'), 'utf8').trim().split('\n').map(JSON.parse).filter(row => row.event === 'call');
      return { task: task.id, arm, frozenCorrect: result.correct, sourceVerifiedCorrect: audit(task.expected, answer, calls),
        cachedInputTokens: result.receipt.usage.reduce((sum, usage) => sum + (usage.cached_input_tokens ?? 0), 0),
        emptyReads: calls.filter(call => call.result?.structuredContent?.results?.some(row => row.status === 'empty')).length };
    }));
    const output = { status: 'POST_HOC_ONLY', reason: 'Frozen exact-string grading rejected unspecified comma whitespace and enum qualification; numbered-text evidence was not handled. Original report preserved. No confirmatory win claimed.', records };
    writeFileSync(join(root, 'audit.json'), JSON.stringify(output, null, 2));
    console.log(JSON.stringify(output, null, 2));
  }
}

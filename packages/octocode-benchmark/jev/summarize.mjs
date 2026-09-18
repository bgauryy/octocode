#!/usr/bin/env node
import { readFileSync, writeFileSync, readdirSync, existsSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const root = dirname(fileURLToPath(import.meta.url));
const run = join(root, 'runs/terra-20260918-v1');
const json = path => JSON.parse(readFileSync(path, 'utf8'));
const sha = value => createHash('sha256').update(value).digest('hex');
const frozen = json(join(root, 'frozen.json'));
const integrity = Object.entries(frozen.hashes).map(([path, hash]) => ({ path, valid: sha(readFileSync(join(root, path))) === hash }));
const rows = [];
for (const arm of ['baseline', 'treatment']) {
  for (const { id } of json(join(root, 'questions.json')).questions) {
    const dir = join(run, arm, id);
    if (!existsSync(dir)) { rows.push({ arm, caseId: id, complete: false }); continue; }
    const events = readdirSync(dir).filter(name => /^event-\d+\.json$/.test(name)).sort().map(name => ({ ...json(join(dir, name)), file: name }));
    const research = events.filter(event => event.kind === 'octocode');
    const jev = events.filter(event => event.kind === 'jev');
    const violations = [];
    let inputTokens = 0, outputTokens = 0, queryRows = 0, toolErrorRows = 0, invalidJsonRequests = 0;
    for (const event of events) {
      const stdout = readFileSync(join(dir, event.file.replace('.json', '.stdout.txt')), 'utf8');
      if (sha(stdout) !== event.stdoutSha256) violations.push(`${event.file}: stdout hash mismatch`);
      if (event.kind === 'octocode') {
        const at = event.args.indexOf('--queries');
        try { const q = JSON.parse(event.args[at + 1]); queryRows += Array.isArray(q) ? q.length : Array.isArray(q.queries) ? q.queries.length : 1; } catch { if (event.exitCode === 2 && stdout.includes('Tool input must be valid JSON.')) invalidJsonRequests++; else violations.push(`${event.file}: query count unavailable`); }
        try { const response = JSON.parse(stdout); toolErrorRows += response.kind === 'octocode.toolError' ? 1 : (response.results || []).filter(row => row.status === 'error' || row.data?.error).length; } catch { toolErrorRows++; }
      }
      if (event.kind === 'jev') {
        const request = json(join(dir, event.file.replace('.json', '.request.json')));
        if (sha(JSON.stringify(request)) !== event.requestSha256) violations.push(`${event.file}: request hash mismatch`);
        try {
          const response = JSON.parse(stdout);
          if (event.exitCode !== 0 || response.model !== 'jev-1.13.0' || !response.answers) violations.push(`${event.file}: invalid Jev response`);
          inputTokens += response.usage?.input_tokens ?? 0;
          outputTokens += response.usage?.output_tokens ?? 0;
          if (!Number.isInteger(response.usage?.input_tokens) || !Number.isInteger(response.usage?.output_tokens)) violations.push(`${event.file}: missing provider usage`);
        } catch { violations.push(`${event.file}: no valid Jev JSON`); }
      }
    }
    if (research.length > 12) violations.push('Research invocation budget exceeded');
    if (jev.length !== (arm === 'baseline' ? 0 : 1)) violations.push('Wrong Jev call count');
    const required = ['result.json', 'answer.md', 'decision-before.json', 'decision-after.json'];
    const complete = required.every(name => existsSync(join(dir, name)));
    if (!complete) violations.push('Missing required answer/decision artifact');
    const result = existsSync(join(dir, 'result.json')) ? json(join(dir, 'result.json')) : {};
    const timestamps = events.flatMap(event => [event.startedAt, event.endedAt]).map(Date.parse).filter(Number.isFinite);
    const wall = timestamps.length ? Math.max(...timestamps) - Math.min(...timestamps) : null;
    const firstLoggedAt = timestamps.length ? new Date(Math.min(...timestamps)).toISOString() : null;
    const answerModifiedAt = existsSync(join(dir, 'answer.md')) ? statSync(join(dir, 'answer.md')).mtime.toISOString() : null;
    rows.push({ arm, caseId: id, complete, valid: violations.length === 0, violations, octocodeCalls: research.length, octocodeQueryRows: queryRows, invalidJsonRequests, toolErrorRows, schemaCalls: events.filter(event => event.kind === 'schema').length, jevCalls: jev.length, jevInputTokens: inputTokens, jevOutputTokens: outputTokens, summedOctocodeMs: research.reduce((sum,e)=>sum+e.elapsedMs,0), summedJevMs: jev.reduce((sum,e)=>sum+e.elapsedMs,0), loggedCaseSpanMs: wall, firstLoggedAt, answerModifiedAt, declaredStartedAt: result.startedAt ?? null, declaredEndedAt: result.endedAt ?? null, hostInputTokens: result.hostInputTokens ?? null, hostOutputTokens: result.hostOutputTokens ?? null, decisionChanged: result.decisionChanged ?? null, adviceApplied: result.adviceApplied ?? null });
  }
}
const sumKeys = ['octocodeCalls','octocodeQueryRows','invalidJsonRequests','toolErrorRows','schemaCalls','jevCalls','jevInputTokens','jevOutputTokens','summedOctocodeMs','summedJevMs'];
const totals = Object.fromEntries(['baseline','treatment'].map(arm => {
  const entries = rows.filter(row => row.arm === arm);
  const start = entries.map(row=>Date.parse(row.firstLoggedAt)).filter(Number.isFinite);
  const end = entries.map(row=>Date.parse(row.answerModifiedAt)).filter(Number.isFinite);
  return [arm, { ...Object.fromEntries(sumKeys.map(key=>[key,entries.reduce((sum,row)=>sum+(row[key]||0),0)])), firstEventToLastAnswerMs: start.length && end.length ? Math.max(...end) - Math.min(...start) : null }];
}));
const result = { suite: frozen.suite, generatedAt: new Date().toISOString(), integrity, allComplete: rows.every(row => row.complete), allProtocolValid: rows.every(row => row.valid) && integrity.every(item => item.valid), rows, totals, limitations: ['Wrapper is not a sandbox.', 'Host tokens unavailable unless actual host telemetry exists.', 'Logged case span excludes time before first and after last tool event; summed tool durations are not total investigation time.'] };
writeFileSync(join(run, 'metrics.json'), JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify(result,null,2));

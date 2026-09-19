#!/usr/bin/env node
// Rank cheap PR titles; retain every required detail read and discovery continuation.
import { spawnSync } from 'node:child_process';
import { existsSync, realpathSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseFlags, print, stop } from './cli-json.mjs';
import { propagateOctocodeEnv } from './octocode-config.mjs';
import { runScout } from './scout.mjs';

function localCliPath() {
  for (const start of [process.cwd(), fileURLToPath(new URL('.', import.meta.url))]) {
    for (let dir = resolve(start); ; dir = dirname(dir)) {
      const candidate = resolve(dir, 'packages/octocode/out/octocode.js');
      if (existsSync(candidate)) return candidate;
      if (dirname(dir) === dir) break;
    }
  }
}

export function callOctocode(tool, query) {
  const local = localCliPath();
  const env = { ...process.env };
  propagateOctocodeEnv({ cwd: process.cwd(), env });
  const args = ['tools', tool, JSON.stringify({ queries: [query] }), '--json'];
  const child = spawnSync(local ? process.execPath : 'npx', local ? [local, ...args] : ['-y', 'octocode', ...args],
    { env, encoding: 'utf8', timeout: 60000, maxBuffer: 10 * 1024 * 1024 });
  let result;
  try { result = JSON.parse(child.stdout); } catch {
    throw new Error('Octocode ' + tool + ' returned no JSON (exit ' + (child.status ?? 'unavailable') + ').');
  }
  if (child.status !== 0) throw new Error('Octocode ' + tool + ' failed: ' + (result.error || result.results?.find(row => row.status === 'error')?.data?.error || 'exit ' + child.status));
  return result;
}

export function runPrTriage(options, { callTool = callOctocode, scout = runScout } = {}) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(options.repo || '')) throw new Error('--repo must be owner/repo.');
  for (const key of ['query', 'question']) if (typeof options[key] !== 'string' || !options[key].trim()) throw new Error('--' + key + ' is required.');
  const limit = Number(options.limit ?? 8);
  if (!Number.isInteger(limit) || limit < 1 || limit > 12) throw new Error('--limit must be an integer from 1 to 12.');
  const [owner, repo] = options.repo.split('/');
  const query = { operation: 'pullRequests', owner, repo, keywords: [options.query], pageSize: limit,
    reasoning: 'Discover PR titles relevant to: ' + options.question };
  const response = callTool('ghSearchHistory', query);
  const row = response?.results?.[0];
  if (response?.error || response?.isError || row?.status === 'error' || row?.data?.error || row?.data?.errorCode) {
    throw new Error('PR discovery failed: ' + (row?.data?.error || response?.error || 'tool error'));
  }
  if (!Array.isArray(row?.data?.pullRequests)) throw new Error('PR discovery returned no pullRequests array.');
  const data = row.data;
  const items = data.pullRequests.map(pr => {
    if (!Number.isInteger(pr.number) || pr.number < 1 || typeof pr.title !== 'string' || !pr.title.trim()) throw new Error('PR discovery returned an invalid number/title row.');
    return { id: 'PR' + pr.number, number: pr.number, source: options.repo + '#' + pr.number, content: pr.title };
  });
  if (items.length > limit || new Set(items.map(item => item.id)).size !== items.length) throw new Error('PR discovery returned too many or duplicate rows.');
  const base = { question: options.question, discovery: { tool: 'ghSearchHistory', query,
    pagination: data.pagination, nextPage: data.next?.nextPage, effectiveQuery: data.effectiveQuery },
    provisional: true, evidenceScope: 'Title-only relevance; never behavior proof or proof of absence.' };
  const detail = item => ({ tool: 'ghGetHistoryItem', query: { operation: 'pullRequest', owner, repo,
    number: item.number, content: { body: true, changedFiles: true },
    reasoning: 'Inspect original PR evidence for: ' + options.question } });
  const out = items.length > 1 ? scout({ claim: options.question, items, taxonomy: 'relevance', model: options.model },
    { dryRun: options.dryRun, output: options.output }) : null;
  if (options.dryRun) return { ...base, status: 'dry-run', candidates: items, request: out?.request,
    requiredReads: [], scoutCalls: 0 };
  const ranked = items.map(item => {
    const result = out ? out.results?.[item.id] : { action: 'gray_read', reason: 'single_candidate' };
    if (!result || !['read', 'gray_read', 'skip'].includes(result.action)) throw new Error('Scout returned no valid verdict for ' + item.id + '.');
    return { ...result, id: item.id, number: item.number, source: item.source, title: item.content };
  }).sort((a, b) => (b.score ?? -1) - (a.score ?? -1));
  return { ...base, status: out ? 'scouted' : items.length ? 'single_candidate' : 'empty', ranked,
    requiredReads: ranked.filter(item => item.action !== 'skip').map(item => ({ ...item, next: detail(item) })),
    model: out?.model, artifacts: out?.artifacts, jev_usage: out?.metrics?.jev, scoutCalls: out ? 1 : 0 };
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log('Usage: node scripts/pr-triage.mjs --repo owner/repo --query "search terms" --question "relevance question" [--limit 1..12] [--model MODEL] [--output DIR] [--dry-run] [--pretty]\nUses Octocode PR discovery; dry-run still searches GitHub but makes no provider call. Follow every requiredReads[].next and discovery.nextPage when needed. Titles never prove behavior or absence.');
    return;
  }
  const flags = parseFlags(argv, ['--repo', '--query', '--question', '--limit', '--model', '--output'], ['--pretty', '--dry-run']);
  print(runPrTriage({ repo: flags['--repo'], query: flags['--query'], question: flags['--question'], limit: flags['--limit'],
    model: flags['--model'], output: flags['--output'], dryRun: flags['--dry-run'] }), flags['--pretty']);
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(resolve(process.argv[1]))).href) {
  try { main(process.argv.slice(2)); } catch (error) { stop(error.message.split('\n')[0], error.exitCode ?? 3); }
}

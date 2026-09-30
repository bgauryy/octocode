#!/usr/bin/env node
// Launch solver agents (headless Claude Code) for each question × arm × pass.
//
//   node run.mjs --run-id <id> [--questions G01,L01|all] [--passes 3] [--arms octocode,rg-gh]
//                [--concurrency 4] [--max-turns 40] [--timeout-min 20] [--model sonnet]
//
// Resumable: a run whose run.json already exists is skipped (delete it to rerun).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import {
  CORPUS_ROOT, RESULTS_DIR, REPO_ROOT, freshCwd, loadQuestions, parseArgs, parseStream, pool,
  readJson, runClaude, sha256, writeJson,
} from './lib.mjs';

const args = parseArgs(process.argv.slice(2), {
  questions: 'all', passes: '3', arms: 'octocode,rg-gh', concurrency: '4',
  'max-turns': '40', 'timeout-min': '20', model: 'sonnet',
});
if (!args['run-id']) throw new Error('--run-id is required');

const runId = String(args['run-id']);
const runDir = path.join(RESULTS_DIR, runId);
const passes = Number(args.passes);
const arms = String(args.arms).split(',');
const maxTurns = String(args['max-turns']);
const timeoutMs = Number(args['timeout-min']) * 60_000;
const model = String(args.model);

const all = loadQuestions();
const questions = args.questions === 'all' ? all : all.filter((q) => String(args.questions).split(',').includes(q.id));

// Every local corpus used by the question set; both arms get read access to exactly these.
const corpusPaths = [...new Set(all.flatMap((q) => q.repos.map((r) => r.path)))].sort();

const MCP_SERVER = path.join(REPO_ROOT, 'packages/octocode-mcp/dist/index.js');
const mcpConfig = {
  mcpServers: {
    octocode: {
      command: 'node',
      args: [MCP_SERVER],
      // The server reads the classification key and other user settings from ~/.octocode itself.
      env: { ENABLE_LOCAL: 'true', ALLOWED_PATHS: corpusPaths.join(','), WORKSPACE_ROOT: CORPUS_ROOT },
    },
  },
};

const ARMS = {
  octocode: {
    toolLine: '',
    flags: (mcpPath) => ['--strict-mcp-config', '--mcp-config', mcpPath, '--tools', '', '--allowedTools', 'mcp__octocode'],
  },
  'rg-gh': {
    toolLine: 'Your available tools are `rg` (ripgrep) and `gh` (the GitHub CLI), run through the Bash tool.',
    flags: () => ['--strict-mcp-config', '--tools', 'Bash', '--allowedTools', 'Bash(rg:*)', 'Bash(gh:*)', '--add-dir', ...corpusPaths],
  },
};

export function buildPrompt(q, arm) {
  const parts = ['Answer the following code-research question.'];
  if (ARMS[arm].toolLine) parts.push(ARMS[arm].toolLine);
  if (q.repos.length) {
    parts.push(
      'Local checkout(s) for this question, read-only, at the listed commits:\n' +
        q.repos.map((r) => `- ${r.repo} @ ${r.sha}: ${r.path}`).join('\n'),
    );
  }
  parts.push(`Question:\n${q.question}`);
  parts.push(
    'Answer with evidence: support each claim with file paths and line numbers, or with commit SHAs, ' +
      'PR/issue numbers or URLs. If you could not find evidence for part of the answer, say so and state ' +
      'your uncertainty instead of guessing.',
  );
  return parts.join('\n\n');
}

function isolationCheck(arm, m) {
  const problems = [];
  const initTools = m.init?.tools ?? [];
  const servers = (m.init?.mcp_servers ?? []).map((s) => `${s.name}:${s.status}`);
  if (arm === 'octocode') {
    if (initTools.some((t) => !t.startsWith('mcp__octocode__'))) problems.push(`non-MCP tools offered: ${initTools.filter((t) => !t.startsWith('mcp__octocode__'))}`);
    if (!servers.includes('octocode:connected')) problems.push(`octocode server not connected: ${servers}`);
    if (m.toolCalls.some((c) => !c.name.startsWith('mcp__octocode__'))) problems.push('non-MCP tool call');
  } else {
    if (initTools.join(',') !== 'Bash') problems.push(`unexpected tools offered: ${initTools}`);
    if (servers.length) problems.push(`MCP servers present: ${servers}`);
    if (m.toolCalls.some((c) => c.name !== 'Bash')) problems.push('non-Bash tool call');
  }
  // Calls outside the allowlist are denied by Claude Code; count them, they are not violations.
  const deniedNonAllowlisted = m.permission_denials.length;
  return { ok: problems.length === 0, problems, initTools, servers, deniedCalls: deniedNonAllowlisted };
}

async function runOne(q, arm, pass, mcpPath) {
  const dir = path.join(runDir, q.id, arm, `pass${pass}`);
  const recordPath = path.join(dir, 'run.json');
  if (fs.existsSync(recordPath)) return readJson(recordPath);
  fs.mkdirSync(dir, { recursive: true });
  const prompt = buildPrompt(q, arm);
  fs.writeFileSync(path.join(dir, 'prompt.txt'), prompt);
  const cwd = freshCwd(`${q.id}-${arm}-p${pass}`);
  const claudeArgs = [
    '-p', prompt, '--model', model, '--setting-sources', '', '--max-turns', maxTurns,
    '--output-format', 'stream-json', '--verbose', ...ARMS[arm].flags(mcpPath),
  ];
  const started = new Date().toISOString();
  const res = await runClaude({ args: claudeArgs, cwd, timeoutMs, streamPath: path.join(dir, 'stream.jsonl') });
  fs.writeFileSync(path.join(dir, 'stderr.txt'), res.stderr);
  fs.rmSync(cwd, { recursive: true, force: true });
  const m = parseStream(res.stream);
  fs.writeFileSync(path.join(dir, 'answer.md'), m.answer ?? '');
  const record = {
    qid: q.id, arm, pass, started, cwdWasFresh: true,
    promptSha256: sha256(prompt),
    exitCode: res.exitCode, timedOut: res.timedOut, wallMs: res.wallMs,
    status: res.timedOut ? 'timeout' : m.resultSubtype === 'success' && !m.isError ? 'ok' : (m.resultSubtype ?? 'no-result'),
    resultSubtype: m.resultSubtype, isError: m.isError,
    usage: m.usage, modelUsage: m.modelUsage, total_cost_usd: m.total_cost_usd,
    duration_ms: m.duration_ms, num_turns: m.num_turns,
    toolCallCount: m.toolCalls.length, toolCounts: m.toolCounts, toolCalls: m.toolCalls,
    toolErrorCount: m.toolErrors.length, toolErrors: m.toolErrors.slice(0, 20),
    permission_denials: m.permission_denials.map((d) => ({ tool: d.tool_name, input: JSON.stringify(d.tool_input).slice(0, 300) })),
    isolation: isolationCheck(arm, m),
    answerChars: (m.answer ?? '').length,
  };
  writeJson(recordPath, record);
  console.log(`[${new Date().toISOString().slice(11, 19)}] ${q.id} ${arm} p${pass}: ${record.status} turns=${record.num_turns} calls=${record.toolCallCount} $${record.total_cost_usd.toFixed(3)} ${(record.wallMs / 1000).toFixed(0)}s${record.isolation.ok ? '' : ' ISOLATION:' + record.isolation.problems.join(';')}`);
  return record;
}

function hashFile(p) {
  try { return sha256(fs.readFileSync(p)); } catch { return null; }
}

async function main() {
  fs.mkdirSync(runDir, { recursive: true });
  const mcpPath = path.join(runDir, 'mcp-config.json');
  writeJson(mcpPath, mcpConfig);

  const manifestPath = path.join(runDir, 'manifest.json');
  const manifest = {
    runId, createdAt: new Date().toISOString(), host: `${os.platform()}-${os.arch()}`,
    claudeVersion: execFileSync('claude', ['--version']).toString().trim(),
    model, maxTurns: Number(maxTurns), timeoutMs, concurrency: Number(args.concurrency), passes, arms,
    questionIds: questions.map((q) => q.id),
    corpus: all.flatMap((q) => q.repos).filter((r, i, a) => a.findIndex((x) => x.path === r.path) === i).map((r) => ({
      ...r, head: execFileSync('git', ['-C', r.path, 'rev-parse', 'HEAD']).toString().trim(),
    })),
    hashes: {
      questionsJson: hashFile(path.join(path.dirname(RESULTS_DIR), 'questions.json')),
      runMjs: hashFile(new URL(import.meta.url).pathname),
      libMjs: hashFile(path.join(path.dirname(RESULTS_DIR), 'lib.mjs')),
      mcpConfig: sha256(JSON.stringify(mcpConfig)),
      mcpServerDist: hashFile(MCP_SERVER),
      userOctocoderc: hashFile(path.join(os.homedir(), '.octocode/.octocoderc')),
      armFlags: Object.fromEntries(Object.entries(ARMS).map(([k, v]) => [k, sha256(JSON.stringify(v.flags('<mcp-config>')))])),
      prompts: Object.fromEntries(questions.flatMap((q) => arms.map((a) => [`${q.id}/${a}`, sha256(buildPrompt(q, a))]))),
    },
  };
  for (const r of manifest.corpus) if (r.head !== r.sha) throw new Error(`corpus ${r.path} is at ${r.head}, expected ${r.sha}`);
  if (fs.existsSync(manifestPath)) {
    const prev = readJson(manifestPath);
    for (const k of ['questionsJson', 'runMjs', 'libMjs', 'mcpConfig', 'mcpServerDist']) {
      if (prev.hashes[k] !== manifest.hashes[k]) throw new Error(`harness input "${k}" changed since this run started; use a new --run-id`);
    }
    manifest.createdAt = prev.createdAt;
    manifest.questionIds = [...new Set([...prev.questionIds, ...manifest.questionIds])];
    manifest.hashes.prompts = { ...prev.hashes.prompts, ...manifest.hashes.prompts };
  }
  writeJson(manifestPath, manifest);

  // Interleave arms and questions so time-of-day drift does not favour one arm.
  const jobs = [];
  for (let pass = 1; pass <= passes; pass++) {
    questions.forEach((q, i) => {
      const order = (i + pass) % 2 === 0 ? arms : [...arms].reverse();
      for (const arm of order) jobs.push(() => runOne(q, arm, pass, mcpPath));
    });
  }
  console.log(`run ${runId}: ${jobs.length} solver runs, concurrency ${args.concurrency}`);
  const records = await pool(jobs, Number(args.concurrency));
  const failed = records.filter((r) => r?.error);
  for (const f of failed) console.error('job error:', f.error);
  const cost = records.reduce((s, r) => s + (r?.total_cost_usd ?? 0), 0);
  console.log(`done: ${records.length - failed.length} records, solver cost $${cost.toFixed(2)}`);
}

main();

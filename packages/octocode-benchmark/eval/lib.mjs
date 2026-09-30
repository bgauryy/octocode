// Shared helpers for the agent-vs-agent eval: spawn headless Claude Code, parse its
// stream-json output, hash inputs, and run jobs with bounded parallelism.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const EVAL_DIR = path.dirname(fileURLToPath(import.meta.url));
export const RESULTS_DIR = path.join(EVAL_DIR, 'results');
export const REPO_ROOT = path.resolve(EVAL_DIR, '../../..');
export const CORPUS_ROOT = path.join(REPO_ROOT, 'octocode-local-testing/repos');

export const sha256 = (s) => createHash('sha256').update(s).digest('hex');

export function loadQuestions() {
  return JSON.parse(fs.readFileSync(path.join(EVAL_DIR, 'questions.json'), 'utf8')).questions;
}

export function parseArgs(argv, defaults = {}) {
  const out = { ...defaults };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) continue;
    const key = a.slice(2);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith('--')) out[key] = true;
    else { out[key] = next; i++; }
  }
  return out;
}

/** A fresh empty working directory outside the repository (no CLAUDE.md, no project memory). */
export function freshCwd(label) {
  return fs.mkdtempSync(path.join(os.tmpdir(), `octocode-eval-${label.replace(/[^\w-]/g, '_')}-`));
}

/**
 * Run `claude -p` and capture the stream-json transcript.
 * Resolves with { stream, stderr, exitCode, timedOut, wallMs }.
 */
export function runClaude({ args, cwd, timeoutMs, streamPath, env = process.env }) {
  return new Promise((resolve) => {
    const start = Date.now();
    const out = fs.createWriteStream(streamPath);
    const child = spawn('claude', args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; child.kill('SIGTERM'); setTimeout(() => child.kill('SIGKILL'), 5000); }, timeoutMs);
    child.stdout.on('data', (d) => { stdout += d; out.write(d); });
    child.stderr.on('data', (d) => { stderr += d; });
    child.on('close', (code) => {
      clearTimeout(timer);
      out.end();
      resolve({ stream: stdout, stderr, exitCode: code, timedOut, wallMs: Date.now() - start });
    });
  });
}

/** Extract the metrics we report from a stream-json transcript. */
export function parseStream(stream) {
  const events = [];
  for (const line of stream.split('\n')) {
    if (!line.trim()) continue;
    try { events.push(JSON.parse(line)); } catch { /* partial line on kill */ }
  }
  const init = events.find((e) => e.type === 'system' && e.subtype === 'init') ?? null;
  const result = [...events].reverse().find((e) => e.type === 'result') ?? null;
  const toolCalls = [];
  const toolErrors = [];
  let lastAssistantText = '';
  for (const e of events) {
    if (e.type === 'assistant') {
      const texts = [];
      for (const c of e.message?.content ?? []) {
        if (c.type === 'tool_use') {
          const call = { name: c.name };
          if (c.name === 'Bash') call.command = String(c.input?.command ?? '').slice(0, 300);
          toolCalls.push(call);
        } else if (c.type === 'text') texts.push(c.text);
      }
      if (texts.length) lastAssistantText = texts.join('\n');
    } else if (e.type === 'user') {
      for (const c of e.message?.content ?? []) {
        if (c.type === 'tool_result' && c.is_error) {
          const text = typeof c.content === 'string' ? c.content : JSON.stringify(c.content);
          toolErrors.push(text.slice(0, 300));
        }
      }
    }
  }
  const toolCounts = {};
  for (const c of toolCalls) {
    const key = c.name === 'Bash' ? `Bash:${/^\s*(rg|gh)\s/.exec(c.command)?.[1] ?? 'other'}` : c.name;
    toolCounts[key] = (toolCounts[key] ?? 0) + 1;
  }
  const usage = result?.usage ?? {};
  return {
    init: init ? { tools: init.tools, mcp_servers: init.mcp_servers, model: init.model, permissionMode: init.permissionMode, slash_commands: init.slash_commands, skills: init.skills, agents: init.agents, cwd: init.cwd } : null,
    answer: result?.result ?? lastAssistantText ?? '',
    resultSubtype: result?.subtype ?? null,
    isError: result?.is_error ?? true,
    usage: {
      input_tokens: usage.input_tokens ?? 0,
      cache_creation_input_tokens: usage.cache_creation_input_tokens ?? 0,
      cache_read_input_tokens: usage.cache_read_input_tokens ?? 0,
      output_tokens: usage.output_tokens ?? 0,
    },
    modelUsage: result?.modelUsage ?? null,
    total_cost_usd: result?.total_cost_usd ?? 0,
    duration_ms: result?.duration_ms ?? 0,
    num_turns: result?.num_turns ?? 0,
    permission_denials: result?.permission_denials ?? [],
    toolCalls,
    toolCounts,
    toolErrors,
  };
}

/** Run async job factories with at most `limit` in flight. */
export async function pool(jobs, limit) {
  const results = new Array(jobs.length);
  let next = 0;
  async function worker() {
    while (next < jobs.length) {
      const i = next++;
      try { results[i] = await jobs[i](); } catch (err) { results[i] = { error: String(err?.stack ?? err) }; }
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, jobs.length) }, worker));
  return results;
}

export function writeJson(file, data) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(data, null, 2) + '\n');
}

export function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

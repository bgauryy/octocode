#!/usr/bin/env node
// Blind judge for a round: one fresh judge session per pass and question; a gap of 2 or more points is graded
// again by a fresh judge with the answers in reverse order.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {
  HarnessError, cli, isMain, loadManifest, parseArgs, passPath, pool, questionText, questions, readTranscript, realGh, runChild, runPath,
  seeded, sessionEnv, shuffled, wrapperBin, removeTreeAndEmptyParents,
} from './lib.mjs';

const HELP = `Usage: node packages/octocode-benchmark/harness/judge.mjs <round manifest> <first|swap> [options]

first  one fresh judge per pass and question. Answers are shown as X, Y, Z… in a seeded random order (kept in
       meta.json), with tool names removed. The judge may read the pinned clones (--add-dir) and use read-only gh.
swap   every question whose first grading has a gap of 2 or more points is graded again in reverse order.

When questions/<set>/answers/<id>.md exists and lists facts (the rows of a table with a "Fact" column, or a
list under a "Facts" heading), the judge also scores each fact per answer as right, missing, wrong or
unsupported; verdict.json then carries "facts" beside the 0-10 "scores". A key without a fact list is given as a
rubric only.

  --pass N       only this pass
  --only A,B     only these questions (<set>-<id>)
  --jobs N       judges at a time (default: the manifest's "jobs")
  --dry-run      write prompt.md and meta.json for each question; start no session

Output: <campaign>/<pass dir>/judge/<set>-<id>[-swap]/{prompt.md, meta.json, judgement.md, verdict.json} and a
line per question in <campaign>/<pass dir>/judge.log. A stored verdict is never overwritten.
Exit codes: 0 every question got a verdict; 1 a judge returned none; 2 bad input.`;

export const LETTERS = 'XYZWVUTS';
export const FACT_STATES = ['right', 'missing', 'wrong', 'unsupported'];
const READ = ['gh', 'jq', 'base64', 'head', 'tail', 'sed', 'grep', 'rg', 'wc', 'sort', 'uniq', 'cut', 'tr', 'cat', 'ls', 'find', 'nl', 'awk', 'git', 'echo', 'python3', 'curl'];

/**
 * The facts an answer key lists, in order: the rows of the table that has a "Fact" column (with a "#" column,
 * only its numbered rows; a "Not the answer" row is a distractor, not a fact), else the list items under "Facts".
 */
export function factsOf(key) {
  const lines = key.split('\n');
  const cells = line => line.trim().replace(/^\||\|$/g, '').split('|').map(cell => cell.trim());
  const isFact = cell => /^fact$/i.test(cell);
  const head = lines.findIndex(line => line.trim().startsWith('|') && cells(line).some(isFact));
  if (head !== -1) {
    const header = cells(lines[head]);
    const number = header.indexOf('#');
    const factAt = header.findIndex(isFact);
    const facts = [];
    for (const line of lines.slice(head + 2)) {
      if (!line.trim().startsWith('|')) break;
      const row = cells(line);
      if (number === -1 ? /^not the answer$/i.test(row[factAt] ?? '') : !/^\d+$/.test(row[number] ?? '')) continue;
      facts.push(row.filter((cell, at) => at !== number && cell).join(': '));
    }
    return facts;
  }
  const heading = lines.findIndex(line => /^#+\s*facts\b/i.test(line));
  if (heading === -1) return [];
  const facts = [];
  for (const line of lines.slice(heading + 1)) {
    if (/^#+\s/.test(line)) break;
    const item = /^\s*(?:[-*]|\d+\.)\s+(.*)$/.exec(line);
    if (item) facts.push(item[1].trim());
    else if (facts.length && /^\s+\S/.test(line)) facts[facts.length - 1] += ` ${line.trim()}`;
  }
  return facts;
}

/** Removes what would tell a judge which arm wrote an answer. */
export function redact(text, { clones, toolNames }) {
  let out = text.replaceAll(clones, '<clones>').replace(/octocode/gi, '[tool]');
  if (toolNames.length) out = out.replace(new RegExp(String.raw`\b(${toolNames.map(name => name.replace(/[.*+?^${}()|[\]\\]/g, String.raw`\$&`)).join('|')})\b`, 'g'), '[tool]');
  return out.replace(/`gh (api|search|pr|issue|repo|release)\b/g, '`[tool] $1');
}

/** The scores and fact states of a judgement, or undefined when it has no complete VERDICT line. */
export function verdictOf(text, letters, factCount) {
  const lines = text.split('\n').map(line => line.trim().replace(/^[`*]+|[`*]+$/g, ''));
  const line = lines.findLast(candidate => candidate.startsWith('VERDICT'));
  if (!line) return undefined;
  const scores = {};
  const depth = {};
  for (const letter of letters) {
    const found = new RegExp(String.raw`\b${letter} c=(\d+(?:\.\d+)?)(?: d=(\d+(?:\.\d+)?))?`).exec(line);
    if (!found) return undefined;
    scores[letter] = Number(found[1]);
    if (found[2] !== undefined) depth[letter] = Number(found[2]);
  }
  let facts;
  if (factCount) {
    facts = {};
    for (const letter of letters) {
      const row = lines.findLast(candidate => new RegExp(String.raw`^FACTS ${letter}\b`).test(candidate));
      const states = Object.fromEntries([...(row ?? '').matchAll(/\bF(\d+)=(\w+)/g)].map(([, n, state]) => [Number(n), state]));
      const complete = Array.from({ length: factCount }, (_, i) => states[i + 1]);
      if (!complete.every(state => FACT_STATES.includes(state))) return undefined;
      facts[letter] = complete;
    }
  }
  return { scores, depth, facts, line };
}

/** The whole judge prompt for one question and answer order. */
export function judgePrompt({ brief, question, id, clones, key, facts, answers }) {
  const letters = LETTERS.slice(0, answers.length);
  const parts = [
    brief, '', question, '',
    `\`<clones>\` in the question and the answers is the directory \`${clones}\` on this machine: pinned, read-only checkouts. Answers had tool names removed (\`[tool]\`). Read-only access only.`,
  ];
  if (key) parts.push('', '## Rubric (a lead to verify, not the truth)', '', key.trim());
  if (facts.length) {
    parts.push(
      '', '## Facts to score', '',
      'After verifying each fact below from the source, score it for every answer: `right` (stated and correct), `missing` (not stated), `wrong` (stated incorrectly), `unsupported` (stated correctly with no evidence that supports it). A fact the source contradicts in the rubric is scored against the source, and you say so.',
      '', ...facts.map((fact, i) => `- F${i + 1}: ${fact}`),
    );
  }
  answers.forEach((answer, i) => parts.push('', `## Answer ${letters[i]}`, '', answer.trim() || '(no answer was produced)'));
  const form = [...letters].map(letter => `${letter} c=<0-10> d=<1-5>`).join(' | ');
  if (facts.length) {
    const states = facts.map((_, i) => `F${i + 1}=<${FACT_STATES.join('|')}>`).join(' ');
    parts.push('', `Before the verdict, write one line per answer, exactly: ${[...letters].map(letter => `\`FACTS ${letter}: ${states}\``).join(' and ')}`);
  }
  parts.push('', `End with exactly one line: VERDICT ${id}: ${form} | winner=<${[...letters].join('|')}|tie> | confident=<yes|no>`);
  return parts.join('\n');
}

/** What one judging needs: the answers in `order`, redacted, and the prompt. */
export function prepare(manifest, pass, question, order) {
  const brief = fs.readFileSync(manifest.judge.brief, 'utf8').split('\n---\n').slice(1).join('\n---\n').trim();
  const toolNames = new Set();
  const answers = order.map(arm => {
    const dir = runPath(manifest, pass, question.set, question.id, arm);
    for (const tool of readTranscript(path.join(dir, 'transcript.jsonl')).tools ?? []) if (tool.startsWith('mcp__')) toolNames.add(tool.split('__').at(-1));
    return fs.readFileSync(path.join(dir, 'answer.md'), 'utf8');
  });
  const key = fs.existsSync(question.key) ? fs.readFileSync(question.key, 'utf8') : undefined;
  const facts = key ? factsOf(key) : [];
  const redaction = { clones: manifest.clones, toolNames: [...toolNames] };
  const prompt = judgePrompt({
    brief,
    question: questionText(question.file, { clones: '<clones>', withHead: false }).trimEnd(),
    id: question.id,
    clones: manifest.clones,
    key,
    facts,
    answers: answers.map(answer => redact(answer, redaction)),
  });
  return { prompt, facts };
}

async function judgeOne(manifest, job, context) {
  const { pass, question, order, out, dryRun } = job;
  if (fs.existsSync(path.join(out, 'verdict.json'))) return { job, ok: true, status: 'skip' };
  fs.mkdirSync(out, { recursive: true });
  const letters = LETTERS.slice(0, order.length);
  const { prompt, facts } = prepare(manifest, pass, question, order);
  fs.writeFileSync(path.join(out, 'prompt.md'), prompt);
  fs.writeFileSync(path.join(out, 'meta.json'), JSON.stringify({ order, facts }));
  if (dryRun) return { job, ok: true, status: `prompt written (${facts.length} facts)` };
  const env = { ...sessionEnv(), GITHUB_TOKEN: context.token, RUN_ID: `judge-p${pass}-${path.basename(out)}`, BENCH_GH_REAL: context.ghReal, BENCH_GH_LOG: path.join(passPath(manifest, pass), 'gh-judge-log.jsonl') };
  env.PATH = `${context.ghBin}${path.delimiter}${env.PATH}`;
  const args = ['-p', prompt, '--model', manifest.models.judge, '--setting-sources', '', '--no-session-persistence', '--strict-mcp-config', '--add-dir', manifest.clones, '--tools', 'Bash', '--allowedTools', ...READ.map(command => `Bash(${command}:*)`), '--output-format', 'json'];
  const started = Date.now();
  let text = '';
  let cost;
  let verdict;
  for (let attempt = 1; attempt <= 2 && !verdict; attempt += 1) {
    const ran = await runChild(context.host, args, { cwd: manifest.neutralCwd, env, timeoutMs: manifest.timeouts.judgeSeconds * 1000 });
    try {
      const reply = JSON.parse(ran.stdout);
      text = reply.result ?? '';
      cost = reply.total_cost_usd;
    } catch (error) {
      text = `judge failed (exit ${ran.code}): ${error.message}`;
    }
    verdict = verdictOf(text, letters, facts.length);
  }
  fs.writeFileSync(path.join(out, 'judgement.md'), text);
  if (!verdict) return { job, ok: false, status: 'NO VERDICT' };
  const perArm = values => values && Object.fromEntries(order.map((arm, i) => [arm, values[letters[i]]]));
  const record = { scores: perArm(verdict.scores), depth: perArm(verdict.depth), facts: perArm(verdict.facts), line: verdict.line, order, cost, wall_s: Math.round((Date.now() - started) / 1000) };
  fs.writeFileSync(path.join(out, 'verdict.json'), JSON.stringify(record));
  return { job, ok: true, status: `${JSON.stringify(record.scores)} | ${verdict.line}` };
}

/** The judging jobs of a mode: pass, then question. */
export function jobs(manifest, mode, { pass, only } = {}) {
  const out = [];
  const passes = pass === undefined ? Array.from({ length: manifest.passes }, (_, i) => i + 1) : [pass];
  for (const n of passes) {
    for (const question of questions(manifest)) {
      if (only && !only.has(question.name)) continue;
      const arms = question.arms.filter(arm => (!manifest.judge.arms || manifest.judge.arms.includes(arm)) && fs.existsSync(path.join(runPath(manifest, n, question.set, question.id, arm), 'answer.md')));
      if (!arms.length) continue;
      const first = path.join(passPath(manifest, n), 'judge', question.name);
      if (mode === 'first') {
        out.push({ pass: n, question, order: shuffled(arms, seeded(`${manifest.seed}-judge-${n}-${question.name}`)), out: first });
      } else if (fs.existsSync(path.join(first, 'verdict.json'))) {
        const verdict = JSON.parse(fs.readFileSync(path.join(first, 'verdict.json'), 'utf8'));
        const scores = Object.values(verdict.scores);
        if (Math.max(...scores) - Math.min(...scores) >= 2) out.push({ pass: n, question, order: verdict.order.toReversed(), out: `${first}-swap` });
      }
    }
  }
  return out;
}

export async function main(argv) {
  const { options, positional } = parseArgs(argv, { flags: ['--dry-run'], values: ['--pass', '--only', '--jobs'] });
  const [file, mode] = positional;
  if (positional.length !== 2 || !['first', 'swap'].includes(mode)) throw new HarnessError('pass a round manifest and "first" or "swap" (see --help)');
  const manifest = loadManifest(file);
  if (!manifest.models.judge) throw new HarnessError('round manifest: "models.judge" is missing');
  const pass = options.pass === undefined ? undefined : Number(options.pass);
  if (pass !== undefined && !(Number.isSafeInteger(pass) && pass >= 1 && pass <= manifest.passes)) throw new HarnessError(`--pass must be 1 to ${manifest.passes}`);
  const parallel = options.jobs === undefined ? manifest.jobs : Number(options.jobs);
  if (!(Number.isSafeInteger(parallel) && parallel >= 1)) throw new HarnessError('--jobs takes a positive integer');
  const dryRun = Boolean(options['dry-run']);
  const todo = jobs(manifest, mode, { pass, only: options.only && new Set(options.only.split(',')) }).map(job => ({ ...job, dryRun }));
  if (!todo.length) {
    console.log(`${mode}: nothing to judge`);
    return 0;
  }
  let context = { host: manifest.host };
  if (!dryRun) {
    const ghReal = realGh();
    const token = spawnSync(ghReal, ['auth', 'token'], { encoding: 'utf8' }).stdout.trim();
    if (!token) throw new HarnessError(`"${ghReal} auth token" returned no token: run gh auth login`);
    context = { ...context, ghReal, token, ghBin: wrapperBin(manifest) };
  }
  let failed = 0;
  try {
    await pool(todo, parallel, job => judgeOne(manifest, job, context), ({ job, ok, status }) => {
      if (!ok) failed += 1;
      const line = `${mode} pass ${job.pass} ${job.question.name} ${status}`;
      console.log(line);
      if (!dryRun) fs.appendFileSync(path.join(passPath(manifest, job.pass), 'judge.log'), `${line}\n`);
    });
  } finally {
    if (context.ghBin) removeTreeAndEmptyParents(context.ghBin, manifest.scratch);
  }
  return failed ? 1 : 0;
}

if (isMain(import.meta.url)) await cli(main, HELP);

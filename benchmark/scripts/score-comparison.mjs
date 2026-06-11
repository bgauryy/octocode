#!/usr/bin/env node
// Generic benchmark scorer.
//
// Combines judge-supplied Q/D scores with per-agent summary.json metrics.
// It does not judge semantic quality; it only makes the arithmetic reproducible.

import { existsSync, readFileSync, writeFileSync } from 'fs';
import { join, basename } from 'path';

const usage = () => `
Usage:
  node benchmark/scripts/score-comparison.mjs --scores scores.json [--questions QUESTIONS.md] [--markdown] [--out out.md] agent=runDir ...

Score file shape:
{
  "drift": [10],
  "exclude": [],
  "init_chars": { "octocode-mcp": 78000 },
  "agents": {
    "octocode": {
      "1": { "Q": 3, "D": 3, "note": "complete" },
      "2": { "quality": 2.5, "depth": 2, "note": "missed one symbol" }
    },
    "gh": {
      "1": [3, 2],
      "2": { "Q": 2, "D": 1 }
    }
  }
}

Notes:
  - Q and D are judge-assigned. This script never fact-checks answers.
  - Character totals come from each run directory's summary.json.
  - approx_tokens = ceil(effective_chars / 4), display-only.
`;

const args = process.argv.slice(2);
let scoresPath = '';
let questionsPath = '';
let markdown = false;
let outPath = '';
const specs = [];

for (let i = 0; i < args.length; i++) {
  const arg = args[i];
  if (arg === '--help' || arg === '-h') {
    process.stdout.write(usage());
    process.exit(0);
  }
  if (arg === '--scores') {
    scoresPath = args[++i] ?? '';
    continue;
  }
  if (arg === '--questions') {
    questionsPath = args[++i] ?? '';
    continue;
  }
  if (arg === '--markdown') {
    markdown = true;
    continue;
  }
  if (arg === '--json') {
    markdown = false;
    continue;
  }
  if (arg === '--out') {
    outPath = args[++i] ?? '';
    continue;
  }
  specs.push(arg);
}

if (!scoresPath || !existsSync(scoresPath) || specs.length < 1) {
  console.error(usage().trim());
  process.exit(1);
}

const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const fmt = n => Number(n).toLocaleString('en', { maximumFractionDigits: 3 });
const round = n => Math.round((Number(n) + Number.EPSILON) * 1000) / 1000;
const approxTokens = chars => Math.ceil(Number(chars || 0) / 4);
const safeDiv = (a, b) => b ? a / b : null;

const parseQuestions = path => {
  const categories = {};
  const drift = new Set();
  if (!path || !existsSync(path)) return { categories, drift };

  const text = readFileSync(path, 'utf8');
  for (const line of text.split('\n')) {
    const match = line.match(/^### Q(\d+)\s+.*$/);
    if (!match) continue;

    const q = Number(match[1]);
    const tags = [...line.matchAll(/\[([^\]]+)\]/g)].map(m => m[1].trim());
    for (const tag of tags) {
      if (tag.toLowerCase() === 'drift') {
        drift.add(q);
        continue;
      }
      if (!categories[q]) {
        categories[q] = tag.split(/[·\s]/)[0].toUpperCase();
      }
    }
  }

  return { categories, drift };
};

const parseSpec = spec => {
  const i = spec.indexOf('=');
  if (i === -1) {
    const runDir = spec;
    return { agent: basename(runDir), runDir };
  }
  return { agent: spec.slice(0, i), runDir: spec.slice(i + 1) };
};

const readRun = ({ agent, runDir }) => {
  const summaryPath = join(runDir, 'summary.json');
  if (!existsSync(summaryPath)) {
    console.error(`score-comparison: missing ${summaryPath}`);
    process.exit(2);
  }
  const summary = readJson(summaryPath);
  const perQ = new Map((summary.per_q ?? []).map(q => [Number(q.q), q]));
  return { agent, runDir, summary, perQ };
};

const normalizeScoresRoot = scores => {
  if (scores.agents && typeof scores.agents === 'object') return scores.agents;
  const ignored = new Set(['drift', 'exclude', 'categories', 'init_chars', 'notes']);
  return Object.fromEntries(Object.entries(scores).filter(([key]) => !ignored.has(key)));
};

const parseScore = (agent, q, raw) => {
  if (Array.isArray(raw)) {
    return { Q: Number(raw[0]), D: Number(raw[1]), note: raw[2] ?? '' };
  }
  if (raw && typeof raw === 'object') {
    return {
      Q: Number(raw.Q ?? raw.q ?? raw.quality),
      D: Number(raw.D ?? raw.d ?? raw.depth),
      note: String(raw.note ?? raw.notes ?? ''),
    };
  }
  console.error(`score-comparison: score for ${agent} Q${q} must include both Q and D`);
  process.exit(3);
};

const validateScore = (agent, q, score) => {
  for (const field of ['Q', 'D']) {
    const value = score[field];
    if (!Number.isFinite(value) || value < 0 || value > 3) {
      console.error(`score-comparison: invalid ${field} for ${agent} Q${q}; expected 0..3`);
      process.exit(3);
    }
  }
};

const scores = readJson(scoresPath);
const scoreAgents = normalizeScoresRoot(scores);
const questionMeta = parseQuestions(questionsPath);
const drift = new Set([
  ...questionMeta.drift,
  ...((scores.drift ?? []).map(Number)),
]);
const exclude = new Set((scores.exclude ?? []).map(Number));
const categories = { ...questionMeta.categories, ...(scores.categories ?? {}) };
const initChars = scores.init_chars ?? {};
const runs = specs.map(parseSpec).map(readRun);

const scoredQuestions = [...new Set(Object.values(scoreAgents).flatMap(agentScores =>
  Object.keys(agentScores ?? {}).map(Number).filter(Number.isFinite)
))].sort((a, b) => a - b);

if (scoredQuestions.length === 0) {
  console.error('score-comparison: no scored questions found');
  process.exit(3);
}

for (const run of runs) {
  if (!scoreAgents[run.agent]) {
    console.error(`score-comparison: scores file has no agent entry for ${run.agent}`);
    process.exit(3);
  }
}

const comparableQuestions = scoredQuestions.filter(q => !exclude.has(q));
const amortizedInit = agent => Number(initChars[agent] ?? 0) / Math.max(comparableQuestions.length, 1);
const winnerWithinTie = values => {
  const sorted = values
    .filter(v => Number.isFinite(v.tradeoff))
    .sort((a, b) => b.tradeoff - a.tradeoff);
  if (sorted.length === 0) return { winner: '—', tie: false };
  if (sorted.length > 1) {
    const diff = sorted[0].tradeoff - sorted[1].tradeoff;
    if (diff / Math.max(sorted[0].tradeoff, 0.01) <= 0.05) {
      return { winner: 'tie', tie: true };
    }
  }
  return { winner: sorted[0].agent, tie: false };
};

const rows = comparableQuestions.map(q => {
  const agents = {};
  for (const run of runs) {
    const rawScore = scoreAgents[run.agent]?.[String(q)] ?? scoreAgents[run.agent]?.[q];
    if (rawScore == null) {
      console.error(`score-comparison: missing score for ${run.agent} Q${q}`);
      process.exit(3);
    }
    const score = parseScore(run.agent, q, rawScore);
    validateScore(run.agent, q, score);

    const metrics = run.perQ.get(q);
    const missing = !metrics || metrics.missing;
    if (missing) {
      console.error(`score-comparison: ${run.agent} is missing metrics for Q${q}`);
      process.exit(4);
    }
    const inChars = Number(metrics?.in_chars ?? 0);
    const outChars = Number(metrics?.out_chars ?? 0);
    const baseChars = Number(metrics?.total_chars ?? (inChars + outChars));
    const effectiveChars = baseChars + amortizedInit(run.agent);
    const researchScore = score.Q * score.D;
    const tradeoff = researchScore / Math.max(effectiveChars / 1000, 0.01);
    const turns = Number(metrics?.lm_turns ?? metrics?.calls ?? 0);
    const turnsPerPoint = turns / Math.max(score.Q, 0.5);
    const actualTokens = Number(metrics?.lm_tokens_in ?? 0) + Number(metrics?.lm_tokens_out ?? 0);

    agents[run.agent] = {
      ...score,
      missing,
      calls: turns,
      in_chars: inChars,
      out_chars: outChars,
      effective_chars: round(effectiveChars),
      approx_tokens: approxTokens(effectiveChars),
      actual_tokens: actualTokens || null,
      tool_elapsed_ms: Number(metrics?.tool_elapsed_ms ?? 0),
      q_elapsed_ms: Number(metrics?.q_elapsed_ms ?? 0),
      reasoning_ms: Number(metrics?.reasoning_ms ?? 0),
      research_score: round(researchScore),
      tradeoff_score: round(tradeoff),
      turns_per_point: round(turnsPerPoint),
    };
  }

  const values = Object.entries(agents).map(([agent, v]) => ({ agent, tradeoff: v.tradeoff_score }));
  const { winner, tie } = drift.has(q) ? { winner: '—', tie: false } : winnerWithinTie(values);
  const bestQ = Math.max(...Object.values(agents).map(v => v.Q));
  const bestD = Math.max(...Object.values(agents).map(v => v.D));
  const cleanWin = winner !== '—' && winner !== 'tie'
    ? agents[winner].Q >= bestQ - 0.5 && agents[winner].D >= bestD - 0.5
    : false;

  return {
    q,
    category: categories[q] ?? '',
    drift: drift.has(q),
    excluded: exclude.has(q),
    agents,
    winner,
    tie,
    clean_win: cleanWin,
  };
});

const totals = {};
for (const run of runs) {
  const agentRows = rows.filter(r => !r.drift && !r.excluded).map(r => r.agents[run.agent]);
  const sum = key => agentRows.reduce((acc, r) => acc + Number(r[key] ?? 0), 0);
  const researchScore = sum('research_score');
  const effectiveChars = sum('effective_chars');
  const actualTokens = sum('actual_tokens');
  totals[run.agent] = {
    questions: agentRows.length,
    Q: round(sum('Q')),
    D: round(sum('D')),
    research_score: round(researchScore),
    calls: round(sum('calls')),
    in_chars: round(sum('in_chars')),
    out_chars: round(sum('out_chars')),
    init_chars: Number(initChars[run.agent] ?? 0),
    effective_chars: round(effectiveChars),
    approx_tokens: approxTokens(effectiveChars),
    actual_tokens: actualTokens || null,
    tradeoff_score: round(researchScore / Math.max(effectiveChars / 1000, 0.01)),
    tradeoff_score_tok: actualTokens ? round(researchScore / Math.max(actualTokens / 1000, 0.01)) : null,
    avg_turns_per_point: round(safeDiv(sum('turns_per_point'), agentRows.length) ?? 0),
    tool_elapsed_ms: round(sum('tool_elapsed_ms')),
    q_elapsed_ms: round(sum('q_elapsed_ms')),
    reasoning_ms: round(sum('reasoning_ms')),
    wins: rows.filter(r => !r.drift && r.winner === run.agent).length,
    ties: rows.filter(r => !r.drift && r.winner === 'tie').length,
    clean_wins: rows.filter(r => !r.drift && r.winner === run.agent && r.clean_win).length,
  };
}

const result = {
  generated_at: new Date().toISOString(),
  scores: scoresPath,
  questions: questionsPath || null,
  agents: runs.map(r => ({ agent: r.agent, runDir: r.runDir })),
  drift: [...drift].sort((a, b) => a - b),
  exclude: [...exclude].sort((a, b) => a - b),
  rows,
  totals,
};

const md = () => {
  const agentNames = runs.map(r => r.agent);
  const perQHeader = ['Q', 'Category', 'Drift', ...agentNames.flatMap(a => [`${a} Q`, `${a} D`, `${a} T`, `${a} chars`, `${a} tradeoff`]), 'Winner', 'Clean'];
  const lines = [
    `# Benchmark Summary — ${agentNames.join(' vs ')}`,
    '',
    '## Per-Question Table',
    '',
    `| ${perQHeader.join(' | ')} |`,
    `| ${perQHeader.map(() => '---').join(' | ')} |`,
  ];

  for (const row of rows) {
    const cells = [
      `Q${row.q}`,
      row.category || '—',
      row.drift ? 'yes' : 'no',
    ];
    for (const agent of agentNames) {
      const v = row.agents[agent];
      cells.push(String(v.Q), String(v.D), String(v.calls), fmt(v.effective_chars), String(v.tradeoff_score));
    }
    cells.push(row.winner, row.clean_win ? 'yes' : row.winner === 'tie' ? 'tie' : 'no');
    lines.push(`| ${cells.join(' | ')} |`);
  }

  lines.push('', '## Totals', '');
  const totalHeader = ['Agent', 'Σ Q', 'Σ D', 'Σ research', 'Calls', 'Effective chars', 'Approx tokens', 'Tradeoff', 'Wins', 'Clean wins'];
  lines.push(`| ${totalHeader.join(' | ')} |`);
  lines.push(`| ${totalHeader.map(() => '---').join(' | ')} |`);
  for (const agent of agentNames) {
    const t = totals[agent];
    lines.push(`| ${agent} | ${t.Q} | ${t.D} | ${t.research_score} | ${t.calls} | ${fmt(t.effective_chars)} | ${fmt(t.approx_tokens)} | ${t.tradeoff_score} | ${t.wins} | ${t.clean_wins} |`);
  }

  lines.push('', '> Drift questions are excluded from totals and wins. Approx tokens are `ceil(effective_chars / 4)` and are display-only unless actual LM token counters are present.');
  return `${lines.join('\n')}\n`;
};

const output = markdown ? md() : `${JSON.stringify(result, null, 2)}\n`;
if (outPath) {
  writeFileSync(outPath, output);
} else {
  process.stdout.write(output);
}

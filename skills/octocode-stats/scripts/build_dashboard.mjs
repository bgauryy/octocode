#!/usr/bin/env node


import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { resolve, dirname, join, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir, platform } from 'node:os';
import { spawn } from 'node:child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const TOKEN_CHAR_RATIO = 4;
const TOKEN_ESTIMATE_NOTE =
  `Token counts are estimated at ~${TOKEN_CHAR_RATIO} chars per token. ` +
  `Actual usage varies by model and content.`;

const SENTINEL = '__OCTOCODE_STATS_DATA__';

const DEFAULTS = {
  stats:
    process.env.OCTOCODE_HOME
      ? join(process.env.OCTOCODE_HOME, 'stats.json')
      : join(homedir(), '.octocode', 'stats.json'),
  output: join(process.cwd(), '.octocode', 'stats', 'dashboard.html'),
  template: resolve(__dirname, '..', 'assets', 'template.html'),
  open: false,
  allowEmpty: false,
};

function parseArgs(argv) {
  const opts = { ...DEFAULTS };
  const args = argv.slice(2);
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    switch (a) {
      case '--stats':
        opts.stats = absolutize(args[++i]);
        break;
      case '--output':
        opts.output = absolutize(args[++i]);
        break;
      case '--template':
        opts.template = absolutize(args[++i]);
        break;
      case '--open':
        opts.open = true;
        break;
      case '--no-open':
        opts.open = false;
        break;
      case '--allow-empty':
        opts.allowEmpty = true;
        break;
      case '-h':
      case '--help':
        printHelp();
        process.exit(0);
        break;
      default:
        if (a.startsWith('--')) {
          die(`Unknown flag: ${a}`);
        }
        die(`Unexpected argument: ${a}`);
    }
  }
  return opts;
}

function absolutize(p) {
  if (!p) die('Missing value for flag');
  return isAbsolute(p) ? p : resolve(process.cwd(), p);
}

function printHelp() {
  process.stdout.write(`build_dashboard.mjs — render Octocode MCP stats as an HTML dashboard

Options:
  --stats <path>     Path to stats.json
                       (default: $OCTOCODE_HOME/stats.json or ~/.octocode/stats.json)
  --output <path>    Output HTML path
                       (default: ./.octocode/stats/dashboard.html)
  --template <path>  HTML template
                       (default: <skill>/assets/template.html)
  --open             Open the dashboard in the default browser after generation
  --no-open          Keep the browser closed (default; accepted for explicitness)
  --allow-empty      Render an empty-state dashboard if stats.json is missing
  -h, --help         Show this help
`);
}

function die(msg) {
  process.stderr.write(`error: ${msg}\n`);
  process.exit(1);
}

function readStats(statsPath, allowEmpty) {
  if (!existsSync(statsPath)) {
    if (!allowEmpty) {
      die(
        `stats.json not found at ${statsPath}\n` +
          `Run any Octocode MCP tool first, or pass --allow-empty to render an empty dashboard.`
      );
    }
    return { present: false, stats: emptyStats() };
  }
  let raw;
  try {
    raw = readFileSync(statsPath, 'utf8');
  } catch (e) {
    die(`failed to read ${statsPath}: ${e.message}`);
  }
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (e) {
    die(`failed to parse JSON in ${statsPath}: ${e.message}`);
  }
  const stats = parsed?.stats ?? parsed;
  if (!stats || typeof stats !== 'object') {
    die(`unexpected stats.json shape — missing "stats" object`);
  }
  return { present: true, stats, version: parsed?.version };
}

function readSessionMeta(statsPath) {
  const sessionPath = join(dirname(statsPath), 'session.json');
  if (!existsSync(sessionPath)) return null;
  try {
    const parsed = JSON.parse(readFileSync(sessionPath, 'utf8'));
    if (!parsed || typeof parsed !== 'object') return null;
    return {
      sessionId: typeof parsed.sessionId === 'string' ? parsed.sessionId : null,
      createdAt: typeof parsed.createdAt === 'string' ? parsed.createdAt : null,
      lastActiveAt:
        typeof parsed.lastActiveAt === 'string' ? parsed.lastActiveAt : null,
      sessionPath,
    };
  } catch {
    return null;
  }
}

function emptyStats() {
  return {
    toolCalls: 0,
    promptCalls: 0,
    errors: 0,
    rateLimits: 0,
    rateLimitsByProvider: {},
    charsSavedByTool: {},
    githubCacheHits: { hits: {}, rateLimits: 0 },
    packageRegistryFailures: {},
  };
}

function safeNumber(n) {
  return typeof n === 'number' && Number.isFinite(n) ? n : 0;
}

function safeRecord(rec) {
  return rec && typeof rec === 'object' ? rec : {};
}

function computeDashboard(stats, ctx) {
  const charsSavedByTool = safeRecord(stats.charsSavedByTool);
  const githubCacheHits = stats.githubCacheHits ?? { hits: {}, rateLimits: 0 };
  const cacheHitsMap = safeRecord(githubCacheHits.hits);
  const rateLimitsByProvider = safeRecord(stats.rateLimitsByProvider);
  const packageRegistryFailures = safeRecord(stats.packageRegistryFailures);
  const totalUsage = stats.totalUsage;

  const toolEntries = Object.entries(charsSavedByTool).map(([name, t]) => {
    const rawChars = safeNumber(t?.rawChars);
    const responseChars = safeNumber(t?.responseChars);
    const savedChars = safeNumber(t?.savedChars);
    const calls = safeNumber(t?.calls);
    return {
      name,
      calls,
      rawChars,
      responseChars,
      savedChars,
      savingsPct: rawChars > 0 ? (savedChars / rawChars) * 100 : 0,
      estimatedTokensSaved: Math.round(savedChars / TOKEN_CHAR_RATIO),
    };
  });

  const aggregated = toolEntries.reduce(
    (acc, t) => {
      acc.rawChars += t.rawChars;
      acc.responseChars += t.responseChars;
      acc.savedChars += t.savedChars;
      acc.charSavingsCalls += t.calls;
      return acc;
    },
    { rawChars: 0, responseChars: 0, savedChars: 0, charSavingsCalls: 0 }
  );

  const rawChars = safeNumber(totalUsage?.rawChars) || aggregated.rawChars;
  const responseChars =
    safeNumber(totalUsage?.responseChars) || aggregated.responseChars;
  const savedChars = safeNumber(totalUsage?.savedChars) || aggregated.savedChars;
  const charSavingsCalls =
    safeNumber(totalUsage?.charSavingsCalls) || aggregated.charSavingsCalls;

  const cacheHitsArr = Object.entries(cacheHitsMap)
    .map(([endpoint, count]) => ({ endpoint, count: safeNumber(count) }))
    .filter((e) => e.count > 0)
    .sort((a, b) => b.count - a.count);

  const cacheHitsTotal =
    safeNumber(totalUsage?.githubCacheHits) ||
    cacheHitsArr.reduce((s, e) => s + e.count, 0);

  const cacheRateLimits =
    safeNumber(totalUsage?.githubCacheRateLimits) ||
    safeNumber(githubCacheHits.rateLimits);

  const rateLimitsByProviderArr = Object.entries(rateLimitsByProvider)
    .map(([provider, count]) => ({ provider, count: safeNumber(count) }))
    .filter((e) => e.count > 0)
    .sort((a, b) => b.count - a.count);

  const packageRegistryFailuresArr = Object.entries(packageRegistryFailures)
    .map(([registry, count]) => ({ registry, count: safeNumber(count) }))
    .filter((e) => e.count > 0)
    .sort((a, b) => b.count - a.count);

  toolEntries.sort((a, b) => b.calls - a.calls || b.savedChars - a.savedChars);
  const savingToolEntries = toolEntries.filter((t) => t.savedChars > 0);
  const remoteToolEntries = toolEntries
    .filter((t) => t.name.startsWith('github') || t.name === 'packageSearch')
    .map((t) => ({
      ...t,
      source:
        t.name === 'packageSearch'
          ? 'package registry'
          : t.name === 'githubCloneRepo'
            ? 'clone payload'
            : 'github provider',
      deltaChars: t.rawChars - t.responseChars,
      sentPctOfRaw: t.rawChars > 0 ? (t.responseChars / t.rawChars) * 100 : 0,
      outcome:
        t.responseChars < t.rawChars
          ? 'reduced'
          : t.responseChars > t.rawChars
            ? 'expanded'
            : 'same',
    }))
    .sort((a, b) => b.calls - a.calls || b.rawChars - a.rawChars);

  const session = ctx.session;
  const sessionAgeMs =
    session?.createdAt && !Number.isNaN(Date.parse(session.createdAt))
      ? Date.now() - Date.parse(session.createdAt)
      : null;

  const totalPackageFailures = packageRegistryFailuresArr.reduce(
    (s, e) => s + e.count,
    0
  );

  return {
    generatedAt: new Date().toISOString(),
    statsPath: ctx.statsPath,
    hasStats: ctx.present,
    schemaVersion: ctx.version ?? null,
    session: session
      ? {
          sessionId: session.sessionId,
          createdAt: session.createdAt,
          lastActiveAt: session.lastActiveAt,
          sessionAgeMs,
          sessionPath: session.sessionPath,
        }
      : null,
    tokenCharRatio: TOKEN_CHAR_RATIO,
    tokenEstimateNote: TOKEN_ESTIMATE_NOTE,
    totals: {
      toolCalls: safeNumber(stats.toolCalls),
      promptCalls: safeNumber(stats.promptCalls),
      errors: safeNumber(stats.errors),
      rateLimits: safeNumber(stats.rateLimits),
      rawChars,
      responseChars,
      savedChars,
      savingsPct: rawChars > 0 ? (savedChars / rawChars) * 100 : 0,
      estimatedTokensSaved: Math.round(savedChars / TOKEN_CHAR_RATIO),
      charSavingsCalls,
      githubCacheHits: cacheHitsTotal,
      githubCacheRateLimits: cacheRateLimits,
      packageRegistryFailures: totalPackageFailures,
      uniqueTools: savingToolEntries.length,
      uniqueCacheEndpoints: cacheHitsArr.length,
      avgCharsSavedPerCall:
        charSavingsCalls > 0 ? Math.round(savedChars / charSavingsCalls) : 0,
    },
    tools: savingToolEntries,
    remoteTools: remoteToolEntries,
    rateLimitsByProvider: rateLimitsByProviderArr,
    githubCacheHits: cacheHitsArr,
    packageRegistryFailures: packageRegistryFailuresArr,
  };
}

function injectData(template, data) {
  if (!template.includes(SENTINEL)) {
    die(
      `template is missing the "${SENTINEL}" sentinel — cannot inject data. ` +
        `Confirm the template file is the one shipped with this skill.`
    );
  }
  const json = JSON.stringify(data).replace(/</g, '\\u003c');
  return template.replace(SENTINEL, json);
}

function ensureDir(filePath) {
  const dir = dirname(filePath);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
}

function openInBrowser(filePath) {
  const url = `file://${filePath}`;
  const plat = platform();
  let cmd, args;
  if (plat === 'darwin') {
    cmd = 'open';
    args = [url];
  } else if (plat === 'win32') {
    cmd = 'cmd';
    args = ['/c', 'start', '""', url];
  } else {
    cmd = 'xdg-open';
    args = [url];
  }
  try {
    const child = spawn(cmd, args, { stdio: 'ignore', detached: true });
    child.unref();
    return true;
  } catch {
    return false;
  }
}

function main() {
  const opts = parseArgs(process.argv);
  const { stats, present, version } = readStats(opts.stats, opts.allowEmpty);

  if (!existsSync(opts.template)) {
    die(`template not found at ${opts.template}`);
  }
  const template = readFileSync(opts.template, 'utf8');

  const sessionMeta = readSessionMeta(opts.stats);
  const data = computeDashboard(stats, {
    statsPath: opts.stats,
    present,
    version,
    session: sessionMeta,
  });
  const html = injectData(template, data);

  ensureDir(opts.output);
  writeFileSync(opts.output, html, 'utf8');

  const t = data.totals;
  process.stdout.write(
    [
      `dashboard:        ${opts.output}`,
      `stats source:     ${opts.stats}${present ? '' : ' (missing — empty state)'}`,
      `tool calls:       ${t.toolCalls}`,
      `tokens saved:     ${t.estimatedTokensSaved.toLocaleString()} (~${t.savingsPct.toFixed(1)}%)`,
      `cache hits:       ${t.githubCacheHits} (avoided ${t.githubCacheRateLimits} rate limits)`,
      `errors:           ${t.errors}`,
      '',
    ].join('\n')
  );

  if (opts.open) {
    if (openInBrowser(opts.output)) {
      process.stdout.write(`opened in default browser.\n`);
    } else {
      process.stdout.write(
        `open the file manually: file://${opts.output}\n`
      );
    }
  } else {
    process.stdout.write(`open manually: file://${opts.output}\n`);
  }
}

main();

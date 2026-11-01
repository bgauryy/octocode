// Later upgrade paths for upgrade-path.mjs: settings from the previous
// octocode-mcp era, rollback to the published versions, re-upgrade, and an
// install without the optional platform package.
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { binPath, excerpt, fallbackVerdict, packageDir } from './upgrade-path-lib.mjs';

const POSIX = process.platform !== 'win32';
const CALLER = 'src/main.ts#main';

function parseJson(text) {
  try {
    return JSON.parse(String(text).trim());
  } catch {
    return null;
  }
}

/** A running server across an npm change to its install: an answer, or a clean failure, never a hang. */
export async function survives(ctx, session) {
  const call = await ctx.mcpLocalSearch(session);
  const exit = await session.close();
  const reply = call.reply ?? {};
  const clean = call.ok || Boolean(reply.error) || Boolean(reply.closed && !reply.closed.signal);
  return {
    ok: !reply.timedOut && clean,
    note: call.ok ? 'answered localSearch' : `no answer: ${excerpt(JSON.stringify(reply), 300)}; exit ${JSON.stringify(exit)}`,
  };
}

/**
 * The new server under settings the previous octocode-mcp documented
 * (ENABLE_LOCAL, ENABLE_CLONE, OCTOCODE_TOKEN, OCTOCODE_OUTPUT_FORMAT,
 * .octocoderc local.enableClone and output.format, renamed tool names in
 * TOOLS_TO_RUN): each is honored or reported.
 */
export async function legacySettings(ctx, mcp) {
  const home = join(ctx.root, 'legacy-home');
  const octocodeHome = join(home, '.octocode');
  mkdirSync(octocodeHome, { recursive: true });
  writeFileSync(join(octocodeHome, '.octocoderc'), `${JSON.stringify({ local: { enableClone: true }, output: { format: 'json' } })}\n`);
  writeFileSync(join(octocodeHome, '.env'), `OCTOCODE_TOKEN=${ctx.DUMMY_TOKEN}\nENABLE_LOCAL=false\n`);
  const homeEnv = { HOME: home, USERPROFILE: home, OCTOCODE_HOME: octocodeHome };
  const view = parseJson(ctx.exec(ctx.nativeBinary(mcp, 'project'), ['config', '--json'], { env: homeEnv }).stdout);
  const diagnostics = (view?.diagnostics ?? []).map(entry => entry.message).join('\n');
  const rcReported = /local\.enableClone/.test(diagnostics) && /output\.format/.test(diagnostics);
  const envFileReported = /OCTOCODE_TOKEN|ENABLE_LOCAL/.test(diagnostics);
  const legacy = await listTools(ctx, mcp, {
    ...homeEnv, ENABLE_LOCAL: 'false', ENABLE_CLONE: 'true', OCTOCODE_TOKEN: ctx.DUMMY_TOKEN, OCTOCODE_OUTPUT_FORMAT: 'yaml',
  });
  const localHidden = !legacy.names.includes('localSearch');
  const envReported = /ENABLE_LOCAL/.test(legacy.stderr);
  // Tool names the previous server listed, in the tool-list settings.
  const renamed = await listTools(ctx, mcp, { ...homeEnv, TOOLS_TO_RUN: 'localSearch,localGetFileContent,ghSearch' });
  const renamedReported = /localGetFileContent/.test(renamed.stderr) && /ghSearch\b/.test(renamed.stderr);
  ctx.current.commands[0].diagnostics = diagnostics;
  return {
    ok: rcReported && envFileReported && (localHidden || envReported) && renamedReported,
    note: `.octocoderc local.enableClone/output.format ${rcReported ? 'reported' : 'silent'}; ` +
      `home .env OCTOCODE_TOKEN/ENABLE_LOCAL ${envFileReported ? 'reported' : 'silent'}; ` +
      `env ENABLE_LOCAL=false → localSearch ${localHidden ? 'hidden' : 'still listed'}${envReported ? ', reported on stderr' : ', nothing on stderr'}; ` +
      `ghCloneRepo ${legacy.names.includes('ghCloneRepo') ? 'listed' : 'not listed'} under ENABLE_CLONE=true; ` +
      `TOOLS_TO_RUN=localSearch,localGetFileContent,ghSearch → [${renamed.names.join(',')}], old names ${renamedReported ? 'reported' : 'not reported'}`,
  };
}

/** tools/list of a new server started with `extraEnv`, recorded in the current step. */
async function listTools(ctx, mcp, extraEnv) {
  const session = ctx.startMcp(mcp, 'project', extraEnv);
  const init = await session.initialize();
  const list = init.result ? await session.request('tools/list', {}) : init;
  await session.close();
  const names = (list.result?.tools ?? []).map(tool => tool.name);
  const shown = Object.entries(extraEnv).filter(([name]) => !/HOME|USERPROFILE|TOKEN/.test(name)).map(([name, value]) => `${name}=${value}`);
  ctx.current.commands.push({ command: `${session.command} (tools/list) with ${shown.join(' ')}`, tools: names, stderr: session.stderr.slice(0, 1500) });
  return { names, stderr: session.stderr };
}

export async function rollbackChecks(ctx, { cli, mcp, bin, from, fromMcp, to, platformName, oldGraph, daemonDelta }) {
  const { step, npm, exec, tgz, dirs } = ctx;
  await step('state.new.write', `${to} writes a setting and a graph for the rollback`, () => {
    const set = exec(bin, ['config', 'set', 'OCTOCODE_REDACT_EMAILS', 'false']);
    const build = ctx.graphBuild(bin);
    return { ok: set.status === 0 && build.status === 0 };
  });
  let running = null;
  if (POSIX) {
    running = ctx.startMcp(mcp, 'project');
    await running.initialize();
  }
  await step('rollback.mcp.install', `npm uninstall ${platformName}; npm install octocode-mcp@${fromMcp} over ${to}${running ? ', new server running' : ''}`, () => {
    const remove = npm(['uninstall', platformName], mcp);
    const install = npm(['install', `octocode-mcp@${fromMcp}`], mcp);
    return { ok: remove.status === 0 && install.status === 0 };
  });
  await step('rollback.mcp.survives', `the running octocode-mcp ${to} answers after npm replaced its binary, or fails clearly`, () => (
    running ? survives(ctx, running) : { skip: true, note: 'Windows: npm cannot replace files of a running server' }
  ));
  await step('rollback.mcp.session', `octocode-mcp@${fromMcp} after rollback: initialize, tools/list, tools/call`, async () => {
    const session = ctx.startMcp(mcp, 'project');
    const result = await ctx.mcpCheck(session);
    await session.close();
    return result;
  });
  await step('rollback.cli.install', `npm uninstall ${platformName}; npm install octocode@${from} over ${to}`, () => {
    const remove = npm(['uninstall', platformName], cli);
    const install = npm(['install', `octocode@${from}`], cli);
    const version = exec(bin, ['--version']);
    return {
      ok: remove.status === 0 && install.status === 0 && version.status === 0 && version.stdout.trim() !== `octocode ${to}`,
      note: `--version '${version.stdout.trim()}'`,
    };
  });
  await step('rollback.cli.state', `octocode@${from} reads the state ${to} wrote`, () => {
    const setting = exec(bin, ['config', 'get', 'OCTOCODE_REDACT_EMAILS']);
    const token = exec(bin, ['config', 'check', 'GH_TOKEN']);
    const search = ctx.cliLocalSearch(bin);
    const query = oldGraph.status === 'PASS' ? ctx.graphCallers(bin) : null;
    const graph = query ? ctx.graphVerdict(query, [CALLER]) : null;
    return {
      ok: setting.stdout.trim().endsWith('false') && token.status === 0 && search.ok && (!graph || graph.ok),
      note: `OCTOCODE_REDACT_EMAILS '${setting.stdout.trim()}'; GH_TOKEN check exit ${token.status}; localSearch ${search.ok ? 'ok' : 'failed'}; ` +
        `${to} graph → ${graph ? `${graph.kind} (exit ${query.status}): ${excerpt(query.stderr || query.stdout, 220)}` : 'n/a'}`,
    };
  });
  await step('daemon.reupgrade', `re-upgrade to ${to}: same bytes, same path, new mtime; earlier ${to} daemons are not reused`, () => {
    const install = npm(['install', tgz('cli'), tgz('platform')], cli);
    if (!POSIX) return { ok: install.status === 0, skip: install.status === 0, note: 'the warm server is unix-only; reinstall only' };
    const before = ctx.listDaemons(dirs.tmp);
    const search = ctx.cliLocalSearch(bin);
    const { reused, started } = daemonDelta(before, ctx.listDaemons(dirs.tmp));
    return {
      ok: install.status === 0 && search.ok && reused.length === 0 && started.length > 0,
      note: `daemons alive before ${before.length} (${before.map(entry => entry.pid).join(',')}); reused ${reused.length}; started ${started.map(entry => entry.pid).join(',') || 'none'}`,
    };
  });
}

export async function omitChecks(ctx, { to, platformName }) {
  const { step, npm, exec, tgz } = ctx;
  await step('registry.cli', `npm install <octocode-${to}.tgz> alone: npm resolves its ${platformName}@${to} pin from the registry`, () => {
    const dir = ctx.newProject('registry-cli');
    const install = npm(['install', tgz('cli')], dir);
    const present = existsSync(packageDir(dir, platformName, 'project'));
    const run = exec(binPath(dir, 'octocode', 'project'), ['--version'], { timeoutMs: 300_000 });
    const verdict = present ? { ok: run.stdout.trim() === `octocode ${to}`, kind: 'installed', note: `--version '${run.stdout.trim()}'` } : fallbackVerdict(run, to, platformName);
    return { ok: install.status === 0 && verdict.ok, note: `${present ? 'registry served the platform package' : 'npm skipped the unresolvable optional pin'}; ${verdict.kind}: ${verdict.note}` };
  });
  await step('omit.cli', `npm install --omit=optional <octocode-${to}.tgz>, then octocode --version`, () => {
    const dir = ctx.newProject('omit-cli');
    const install = npm(['install', '--omit=optional', tgz('cli')], dir);
    const present = existsSync(packageDir(dir, platformName, 'project'));
    const verdict = fallbackVerdict(exec(binPath(dir, 'octocode', 'project'), ['--version'], { timeoutMs: 300_000 }), to, platformName);
    return { ok: install.status === 0 && !present && verdict.ok, note: `${present ? 'platform package installed anyway; ' : ''}${verdict.kind}: ${verdict.note}` };
  });
  await step('omit.mcp', `npm install --omit=optional <octocode-mcp-${to}.tgz>, then MCP initialize`, async () => {
    const dir = ctx.newProject('omit-mcp');
    const install = npm(['install', '--omit=optional', tgz('mcp')], dir);
    const session = ctx.startMcp(dir, 'project');
    const started = Date.now();
    const init = await session.initialize(300_000);
    const exit = await session.close();
    const message = init.error?.message ?? '';
    const fetched = init.result?.serverInfo?.version === to;
    const clear = !init.timedOut && message.includes(platformName) && /not installed/i.test(message);
    ctx.current.commands.push({ command: `${session.command} (initialize)`, reply: init, exit, stderr: session.stderr.slice(0, 1500) });
    return {
      ok: install.status === 0 && (fetched || clear),
      note: `${fetched ? 'registry fallback fetched the binary' : clear ? 'initialize answered with a clear error' : 'unclear'} in ${Date.now() - started} ms: ${excerpt(message || JSON.stringify(init), 300)}`,
    };
  });
}

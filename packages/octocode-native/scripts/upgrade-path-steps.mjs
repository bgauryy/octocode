// The upgrade paths upgrade-path.mjs checks, in order. They share one
// isolated home, so state written by one version is read by the next.
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { binPath, compareText, excerpt, packageDir } from './upgrade-path-lib.mjs';
import { legacySettings, omitChecks, rollbackChecks, survives } from './upgrade-path-rollback.mjs';

const require = createRequire(import.meta.url);
const { platformPackageName } = require('../bin/platform.cjs');

const POSIX = process.platform !== 'win32';
const CALLER = 'src/main.ts#main';
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

function parseJson(text) {
  try {
    return JSON.parse(String(text).trim());
  } catch {
    return null;
  }
}

/** The launcher a `.bin`/global shim runs: `octocode/out/octocode.js`, not another package's bin. */
function shimTarget(bin) {
  const wanted = join('octocode', 'out', 'octocode.js');
  if (!existsSync(bin)) return { ok: false, note: `${bin} missing` };
  if (!POSIX) {
    const text = readFileSync(bin, 'utf8');
    return { ok: text.includes(wanted), note: `shim runs ${/"%dp0%\\([^"]+)"/.exec(text)?.[1] ?? '?'}` };
  }
  const target = realpathSync(bin);
  return { ok: target.endsWith(wanted), note: `shim → ${target}` };
}

/** Daemons whose socket answered a request since `before` was taken, and the ones started since. */
function daemonDelta(before, now) {
  const reused = before.filter(old => now.some(entry => entry.socket === old.socket && entry.socketMtimeMs !== old.socketMtimeMs));
  const started = now.filter(entry => !before.some(old => old.pid === entry.pid));
  return { reused, started };
}

/** Top-level cache directories under the home's `tmp/`. */
function cacheDirs(files) {
  return [...new Set(files.filter(file => file.startsWith('tmp/')).map(file => file.split('/')[1]))].toSorted(compareText);
}

function useCheck(ctx, bin) {
  const version = ctx.exec(bin, ['--version']);
  const schema = ctx.exec(bin, ['schema']);
  const search = ctx.cliLocalSearch(bin);
  const failure = schema.status ? `: ${excerpt(schema.stdout + schema.stderr, 200)}` : '';
  return {
    version: version.stdout.trim(),
    ok: version.status === 0 && schema.status === 0 && schema.stdout.includes('localSearch') && search.ok,
    note: `--version '${version.stdout.trim()}' (exit ${version.status}); schema exit ${schema.status}${failure}; localSearch ${search.ok ? 'ok' : 'failed'}`,
  };
}

/** The new CLI reads the home `.env` and `.octocoderc` and never prints the stored token. */
function configReadback(ctx, bin) {
  const check = ctx.exec(bin, ['config', 'check', 'GH_TOKEN']);
  const token = ctx.exec(bin, ['config', 'get', 'GH_TOKEN']);
  const setting = parseJson(ctx.exec(bin, ['config', 'get', 'OCTOCODE_REDACT_EMAILS', '--json']).stdout);
  const view = parseJson(ctx.exec(bin, ['config', '--json']).stdout);
  const leaked = `${token.stdout}${token.stderr}`.includes(ctx.DUMMY_TOKEN);
  const errors = (view?.diagnostics ?? []).filter(entry => entry.severity === 'error');
  const keys = view?.configFile?.exists ? view.configFile.keys ?? [] : [];
  const tokenOk = check.status === 0 && token.status === 0 && !leaked;
  return {
    ok: tokenOk && setting?.value === 'true' && keys.includes('tools') && errors.length === 0,
    note: `token: check exit ${check.status}, get '${token.stdout.trim().split(' ')[0]}'${leaked ? ' (LEAKED)' : ''}; ` +
      `OCTOCODE_REDACT_EMAILS=${setting?.value}; .octocoderc keys ${JSON.stringify(keys)}; ${errors.length} config errors`,
  };
}

/** Path 1 and 4 (before): the published CLI as a project dependency writes state. */
async function oldCliPhase(ctx, run) {
  const { step, npm, exec } = ctx;
  const { from, cli, bin } = run;
  const installed = await step('cli.old.install', `npm install octocode@${from}`, () => ({ ok: npm(['install', `octocode@${from}`], cli).status === 0 }));
  if (installed.status !== 'PASS') throw new Error(`octocode@${from} did not install`);
  await step('cli.old.use', `octocode@${from}: --version, schema, localSearch`, () => useCheck(ctx, bin), { required: false });
  await step('state.old.config', `octocode@${from} writes the home .env (dummy token, a setting) and .octocoderc`, () => {
    const token = exec(bin, ['config', 'set', 'GH_TOKEN', '--stdin'], { input: `${ctx.DUMMY_TOKEN}\n` });
    const setting = exec(bin, ['config', 'set', 'OCTOCODE_REDACT_EMAILS', 'true']);
    mkdirSync(ctx.env.OCTOCODE_HOME, { recursive: true });
    writeFileSync(join(ctx.env.OCTOCODE_HOME, '.octocoderc'), `${JSON.stringify({ tools: { disabled: ['ghSearchRepo'] } })}\n`);
    const view = exec(bin, ['config', '--json']);
    return { ok: token.status === 0 && setting.status === 0, note: `config --json exit ${view.status}` };
  });
  run.oldCache = await step('state.old.cache', `octocode@${from} fills its response cache (npm registry read)`, () => {
    const result = ctx.artifactSearch(bin);
    return { ok: result.status === 0, note: `artifactSearch exit ${result.status}` };
  }, { required: false });
  run.oldGraph = await step('state.old.graph', `octocode@${from} builds a code graph`, () => {
    if (exec(bin, ['schema', 'graphBuild']).status !== 0) return { skip: true, note: 'no graphBuild tool' };
    const build = ctx.graphBuild(bin);
    const ids = ctx.graphResultIds(ctx.graphCallers(bin).stdout);
    return { ok: build.status === 0 && Boolean(ids?.includes(CALLER)), note: `callers: ${JSON.stringify(ids)}` };
  });
  await step('state.old.daemon', `octocode@${from} binary runs a warm \`serve\` daemon in the shared socket dir`, async () => {
    if (!POSIX) return { skip: true, note: 'the warm server is unix-only' };
    const native = ctx.nativeBinary(cli, 'project');
    if (exec(native, ['serve', '--help']).status !== 0) return { skip: true, note: 'old binary has no serve' };
    const daemon = ctx.startOldDaemon(native);
    run.oldDaemon = daemon;
    for (let tries = 0; tries < 50 && !existsSync(daemon.socket); tries += 1) await sleep(100);
    const live = ctx.listDaemons(ctx.dirs.tmp).some(entry => entry.pid === daemon.child.pid);
    return { ok: live && existsSync(daemon.socket), note: `pid ${daemon.child.pid}, ${daemon.socket}` };
  }, { required: false });
}

/** Path 1 and 4 (after): upgrade the project dependency in place and read the old state. */
async function upgradeCliPhase(ctx, run) {
  const { step, npm, exec, tgz } = ctx;
  const { from, to, cli, bin, platformName } = run;
  const homeBefore = ctx.listFiles(ctx.env.OCTOCODE_HOME);
  const daemonsBefore = ctx.listDaemons(ctx.dirs.tmp);
  await step('cli.upgrade.install', `npm install <octocode-${to}.tgz> <${platformName}-${to}.tgz> over octocode@${from}`, () => {
    const install = npm(['install', tgz('cli'), tgz('platform')], cli);
    const shim = shimTarget(bin);
    const stale = existsSync(packageDir(cli, '@octocodeai/octocode-native', 'project'));
    return { ok: install.status === 0 && shim.ok && !stale, note: `${shim.note}${stale ? '; @octocodeai/octocode-native is still installed' : ''}` };
  });
  await step('cli.new.use', `octocode ${to}: --version, schema, localSearch`, () => {
    const result = useCheck(ctx, bin);
    return { ok: result.ok && result.version === `octocode ${to}`, note: result.note };
  });
  await step('state.new.config', `${to} reads the .env and .octocoderc that ${from} wrote`, () => configReadback(ctx, bin));
  await step('state.new.home', `${to} keeps and reuses what ${from} wrote under the home`, () => {
    const cache = run.oldCache.status === 'PASS' ? ctx.artifactSearch(bin) : null;
    const after = ctx.listFiles(ctx.env.OCTOCODE_HOME);
    const lost = homeBefore.filter(file => !after.includes(file) && !/\.lock$|\.tmp$|(^|\/)locks\//.test(file));
    const home = exec(bin, ['config', 'home']).stdout.trim();
    return {
      ok: lost.length === 0 && (!cache || cache.status === 0) && home === ctx.env.OCTOCODE_HOME,
      note: `home ${home}; ${homeBefore.length} files before, ${after.length} after, lost ${JSON.stringify(lost)}${cache ? `; artifactSearch exit ${cache.status}` : ''}; ` +
        `tmp/ entries ${JSON.stringify(cacheDirs(homeBefore))} → ${JSON.stringify(cacheDirs(after))}`,
      data: { before: homeBefore, after },
    };
  });
  await step('state.new.graph', `${to} queries the ${from} graph: same answer or a clear rebuild error`, () => {
    if (run.oldGraph.status !== 'PASS') return { skip: true, note: 'no old graph' };
    const query = ctx.graphCallers(bin);
    const verdict = ctx.graphVerdict(query, [CALLER]);
    const build = ctx.graphBuild(bin);
    const fresh = ctx.graphResultIds(ctx.graphCallers(bin).stdout);
    return {
      ok: verdict.ok && build.status === 0 && fresh?.join() === CALLER,
      note: `old snapshot → ${verdict.kind} (exit ${query.status}): ${excerpt(query.stderr || query.stdout, 220)}; rebuild exit ${build.status}, callers ${JSON.stringify(fresh)}`,
    };
  });
  await step('state.new.daemon', `${to} CLI calls never reach a daemon started before the upgrade`, async () => {
    if (!POSIX) return { skip: true, note: 'the warm server is unix-only' };
    const search = ctx.cliLocalSearch(bin);
    const { reused, started } = daemonDelta(daemonsBefore, ctx.listDaemons(ctx.dirs.tmp));
    const oldExit = run.oldDaemon ? await Promise.race([run.oldDaemon.exit, sleep(10).then(() => null)]) : null;
    const old = oldExit ? `exited ${JSON.stringify(oldExit)}` : run.oldDaemon ? 'still running, untouched' : 'n/a';
    return {
      ok: search.ok && reused.length === 0 && started.length > 0 && !oldExit?.signal,
      note: `pre-upgrade daemons ${daemonsBefore.length}, reused ${reused.length}; started since ${started.map(entry => entry.pid).join(',') || 'none'}; old daemon ${old}`,
    };
  });
}

/** Path 2 (before): the published MCP server, left running across its upgrade. */
async function mcpOldPhase(ctx, { fromMcp, mcp }) {
  await ctx.step('mcp.old.install', `npm install octocode-mcp@${fromMcp}`, () => ({ ok: ctx.npm(['install', `octocode-mcp@${fromMcp}`], mcp).status === 0 }));
  const session = ctx.startMcp(mcp, 'project');
  await ctx.step('mcp.old.session', `octocode-mcp@${fromMcp}: initialize, tools/list, tools/call localSearch`, () => ctx.mcpCheck(session), { required: false });
  return session;
}

/** Path 2: upgrade octocode-mcp while the old server runs (Windows locks running files: stop it first). */
async function mcpUpgradePhase(ctx, run, oldSession) {
  const { step, npm, tgz } = ctx;
  const { fromMcp, to, mcp, platformName } = run;
  if (!POSIX) await oldSession.close();
  await step('mcp.upgrade.install', `npm install <octocode-mcp-${to}.tgz> <${platformName}-${to}.tgz> over octocode-mcp@${fromMcp}${POSIX ? ', old server running' : ''}`, () => ({
    ok: npm(['install', tgz('mcp'), tgz('platform')], mcp).status === 0,
  }));
  await step('mcp.old.survives', `the running octocode-mcp@${fromMcp} answers after npm replaced its files, or fails clearly`, () => (
    POSIX ? survives(ctx, oldSession) : { skip: true, note: 'Windows: npm cannot replace files of a running server; it was stopped first' }
  ));
  await step('mcp.new.session', `octocode-mcp ${to}: initialize, tools/list, tools/call; home .octocoderc applies`, async () => {
    const session = ctx.startMcp(mcp, 'project');
    const result = await ctx.mcpCheck(session, to);
    await session.close();
    const rc = !result.names.includes('ghSearchRepo');
    return { ok: result.ok && rc, note: `${result.note}; .octocoderc tools.disabled ${rc ? 'applied' : 'NOT applied'}` };
  });
  await step('mcp.legacy.settings', `${to} with ${fromMcp}-era settings: honored or reported`, () => legacySettings(ctx, mcp), { required: false });
}

/** Path 3: global installs with a temp prefix, and npx. */
async function globalPhase(ctx, run) {
  const { step, npm, exec, tgz, NPX } = ctx;
  const { from, fromMcp, to, platformName } = run;
  const prefix = ctx.dirs.globalPrefix;
  const gbin = binPath(prefix, 'octocode', 'global');
  await step('global.old', `npm install -g --prefix <tmp> octocode@${from} octocode-mcp@${fromMcp}`, () => {
    const install = npm(['install', '-g', '--prefix', prefix, `octocode@${from}`, `octocode-mcp@${fromMcp}`], ctx.root);
    const version = exec(gbin, ['--version']);
    return { ok: install.status === 0 && version.status === 0, note: `--version '${version.stdout.trim()}'` };
  });
  await step('global.upgrade', `npm install -g --prefix <tmp> <octocode, octocode-mcp, ${platformName} ${to} tarballs>`, async () => {
    const install = npm(['install', '-g', '--prefix', prefix, tgz('cli'), tgz('mcp'), tgz('platform')], ctx.root);
    const use = useCheck(ctx, gbin);
    const shim = shimTarget(gbin);
    const session = ctx.startMcp(prefix, 'global');
    const result = await ctx.mcpCheck(session, to);
    await session.close();
    return {
      ok: install.status === 0 && use.ok && use.version === `octocode ${to}` && shim.ok && result.ok,
      note: `${use.note}; ${shim.note}; MCP ${result.note}`,
    };
  });
  await step('npx.old', `npx -y octocode@${from} --version`, () => {
    const version = exec(NPX, ['-y', `octocode@${from}`, '--version'], { timeoutMs: 600_000 });
    return { ok: version.status === 0, note: `'${version.stdout.trim()}'` };
  }, { required: false });
  await step('npx.new', `npx -y -p <octocode-${to}.tgz> -p <platform.tgz> octocode; npx octocode-mcp session`, async () => {
    const npx = ['-y', '-p', tgz('cli'), '-p', tgz('platform'), 'octocode'];
    const version = exec(NPX, [...npx, '--version'], { timeoutMs: 600_000 });
    const schema = exec(NPX, [...npx, 'schema'], { timeoutMs: 600_000 });
    const session = ctx.mcpCommand(NPX, ['-y', '-p', tgz('mcp'), '-p', tgz('platform'), 'octocode-mcp']);
    const result = await ctx.mcpCheck(session, to);
    await session.close();
    return {
      ok: version.stdout.trim() === `octocode ${to}` && schema.status === 0 && result.ok,
      note: `--version '${version.stdout.trim()}'; schema exit ${schema.status}; MCP ${result.note}`,
    };
  });
}

export async function pathChecks(ctx) {
  const cli = ctx.newProject('cli');
  const run = {
    from: ctx.report.from.octocode,
    fromMcp: ctx.report.from['octocode-mcp'],
    to: ctx.tarballs.version,
    platformName: platformPackageName(ctx.suffix),
    cli,
    bin: binPath(cli, 'octocode', 'project'),
    mcp: ctx.newProject('mcp'),
  };
  await oldCliPhase(ctx, run);
  const oldSession = await mcpOldPhase(ctx, run);
  await upgradeCliPhase(ctx, run);
  await mcpUpgradePhase(ctx, run, oldSession);
  await globalPhase(ctx, run);
  await rollbackChecks(ctx, { ...run, daemonDelta });
  await omitChecks(ctx, run);
  await ctx.step('brew', 'Homebrew formula (RELEASE.md row 7)', () => ({ skip: true, note: 'not shipped: the tap still serves the Node-based 2.0.0 formula' }));
}

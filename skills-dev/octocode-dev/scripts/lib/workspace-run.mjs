// Workspace script execution: sequential and dependency-parallel `yarn workspace` runs,
// and the full repo `verify` gate.
import { spawn, spawnSync } from 'node:child_process';
import {
  ROOT,
  SKILL_SCRIPT_POLICY,
  collectBuildDependencies,
  getWorkspaceMap,
  topologicallySort,
} from './workspaces.mjs';
import { checkBuildOutputs, checkLayering, checkRequiredScripts } from './workspace-checks.mjs';

function runCommand(command, args, cwd = ROOT) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit',
    env: process.env,
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function resolveScript(workspace, scriptName, preferScript) {
  const scripts = workspace.packageJson.scripts || {};
  return preferScript && scripts[preferScript] ? preferScript : scriptName;
}

export function runWorkspaceScript(workspaces, scriptName, preferScript) {
  const eligibleWorkspaces = workspaces.filter(workspace =>
    workspace.requiredScripts.includes(scriptName)
  );

  const orderedWorkspaces = topologicallySort(eligibleWorkspaces);

  for (const workspace of orderedWorkspaces) {
    const script = resolveScript(workspace, scriptName, preferScript);
    console.log(`\n==> ${workspace.location}: ${script}`);
    runCommand('yarn', ['workspace', workspace.name, 'run', script]);
  }
}

function runPrefixed(label, command, args) {
  return new Promise(resolve => {
    const child = spawn(command, args, { cwd: ROOT, env: process.env, stdio: ['ignore', 'pipe', 'pipe'] });
    for (const stream of [child.stdout, child.stderr]) {
      let pending = '';
      stream.setEncoding('utf8');
      stream.on('data', chunk => {
        const lines = (pending + chunk).split('\n');
        pending = lines.pop();
        for (const line of lines) process.stdout.write(`[${label}] ${line}\n`);
      });
      stream.on('end', () => pending && process.stdout.write(`[${label}] ${pending}\n`));
    }
    child.on('error', error => resolve({ status: 1, error }));
    child.on('close', status => resolve({ status: status ?? 1 }));
  });
}

/**
 * Runs each workspace as soon as its dependencies (package + BUILD_INPUTS
 * edges) finished. Independent packages — the separate Cargo workspaces
 * (native) each own a target dir, so
 * they never contend on a Cargo lock — overlap. After a failure no new task
 * starts; running ones finish, then the run exits non-zero.
 */
export async function runWorkspaceScriptParallel(workspaces, scriptName, preferScript) {
  const eligibleWorkspaces = workspaces.filter(workspace =>
    workspace.requiredScripts.includes(scriptName)
  );
  const workspaceMap = getWorkspaceMap(eligibleWorkspaces);
  const results = new Map();
  let failed = false;
  const started = Date.now();

  for (const workspace of topologicallySort(eligibleWorkspaces)) {
    const dependencies = [...collectBuildDependencies(workspace, workspaceMap)].map(name => results.get(name));
    results.set(
      workspace.name,
      Promise.all(dependencies).then(async () => {
        if (failed) return { workspace, skipped: true };
        const script = resolveScript(workspace, scriptName, preferScript);
        const taskStarted = Date.now();
        console.log(`==> ${workspace.location}: ${script}`);
        const { status, error } = await runPrefixed(workspace.location, 'yarn', ['workspace', workspace.name, 'run', script]);
        const seconds = ((Date.now() - taskStarted) / 1000).toFixed(1);
        if (status !== 0) {
          failed = true;
          console.error(`<== ${workspace.location}: ${script} FAILED (${error?.message ?? `exit ${status}`}) after ${seconds}s`);
        } else {
          console.log(`<== ${workspace.location}: ${script} ok in ${seconds}s`);
        }
        return { workspace, status, seconds };
      })
    );
  }

  const outcomes = await Promise.all(results.values());
  console.table(
    outcomes.map(({ workspace, status, seconds, skipped }) => ({
      workspace: workspace.location,
      result: skipped ? 'skipped' : status === 0 ? 'ok' : 'FAILED',
      seconds: seconds ?? '-',
    }))
  );
  console.log(`${scriptName}: ${((Date.now() - started) / 1000).toFixed(1)}s wall`);
  if (failed) process.exit(1);
}

export function runVerify(workspaces) {
  checkRequiredScripts(workspaces);
  checkLayering(workspaces);
  runCommand('node', ['--test', 'skills-dev/octocode-dev/scripts/*.test.mjs']);
  runCommand('node', ['skills-dev/octocode-dev/scripts/dedupe-deps.mjs']);
  runCommand('node', ['skills-dev/octocode-dev/scripts/docs-verify.mjs']);

  // A workspace with a verify script owns its complete gate; the rest run build/lint/test.
  // Dependency order (package deps + BUILD_INPUTS) keeps generated inputs built first.
  for (const workspace of topologicallySort(workspaces)) {
    const scripts = workspace.packageJson.scripts?.verify
      ? ['verify']
      : SKILL_SCRIPT_POLICY.filter(name => workspace.requiredScripts.includes(name));
    for (const script of scripts) {
      console.log(`\n==> ${workspace.location}: ${script}`);
      runCommand('yarn', ['workspace', workspace.name, 'run', script]);
    }
  }

  checkBuildOutputs(workspaces);
  // After the builds: it validates the research skill's examples against the built CLI's live schemas.
  runCommand('node', ['skills-dev/octocode-dev/scripts/check-research-guidance.mjs']);
}

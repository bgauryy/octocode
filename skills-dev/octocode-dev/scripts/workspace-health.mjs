#!/usr/bin/env node

import { discoverWorkspaces } from './lib/workspaces.mjs';
import { checkBuildOutputs, checkLayering, checkRequiredScripts } from './lib/workspace-checks.mjs';
import { runVerify, runWorkspaceScript, runWorkspaceScriptParallel } from './lib/workspace-run.mjs';

function printScriptMatrix(workspaces) {
  const rows = workspaces.map(workspace => {
    const availableScripts = workspace.packageJson.scripts || {};
    const missingScripts = workspace.requiredScripts.filter(
      scriptName => !availableScripts[scriptName]
    );

    return {
      workspace: workspace.location,
      kind: workspace.kind,
      required: workspace.requiredScripts.join(', '),
      missing: missingScripts.length > 0 ? missingScripts.join(', ') : 'none',
    };
  });

  console.table(rows);
}

async function main() {
  const args = process.argv.slice(2);
  const excludes = [];
  const positional = [];
  let parallel = false;
  let preferScript;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--exclude' && i + 1 < args.length) {
      excludes.push(args[++i]);
    } else if (args[i] === '--parallel') {
      parallel = true;
    } else if (args[i] === '--prefer' && i + 1 < args.length) {
      preferScript = args[++i];
    } else {
      positional.push(args[i]);
    }
  }
  const [mode = 'report', scriptName] = positional;

  let workspaces = discoverWorkspaces();
  if (excludes.length > 0) {
    workspaces = workspaces.filter(ws => !excludes.includes(ws.name));
  }

  switch (mode) {
    case 'report':
      printScriptMatrix(workspaces);
      return;
    case 'check':
      checkRequiredScripts(workspaces);
      checkLayering(workspaces);
      console.log('Workspace contract check passed.');
      return;
    case 'check-outputs':
      checkBuildOutputs(workspaces);
      console.log('Build outputs verified.');
      return;
    case 'run':
      if (!scriptName) {
        console.error(
          'Usage: node skills-dev/octocode-dev/scripts/workspace-health.mjs run <script> [--parallel] [--prefer <script>] [--exclude <workspace>]'
        );
        process.exit(1);
      }
      checkRequiredScripts(workspaces);
      if (parallel) await runWorkspaceScriptParallel(workspaces, scriptName, preferScript);
      else runWorkspaceScript(workspaces, scriptName, preferScript);
      return;
    case 'verify':
      runVerify(workspaces);
      return;
    default:
      console.error(`Unknown workspace-health mode: ${mode}`);
      process.exit(1);
  }
}

await main();

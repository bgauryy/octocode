#!/usr/bin/env node
import { isMainModule } from '../skills/octocode-graph/scripts/lib/entrypoint.mjs';
import { delegateToNative, resolveNativeBin } from 'octocode/native';

export async function runGraphCli(args) {
  const engine = await resolveNativeBin();
  if ('error' in engine) throw new Error(engine.error);
  // Installing the graph server opts into the native graph tools. Workspace restrictions
  // and the host's enabled/disabled tool lists still apply to every MCP request.
  const env = args.includes('mcp') && process.env.OCTOCODE_BETA === undefined
    ? { ...process.env, OCTOCODE_BETA: 'true' }
    : process.env;
  return delegateToNative(engine.bin, ['__graph', ...args], env);
}

if (isMainModule(import.meta.url)) {
  try {
    process.exitCode = await runGraphCli(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}

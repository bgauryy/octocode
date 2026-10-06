import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { configuredOctocodeServer, isOctocodeTool, octocodeMcpEnabled, octocodeToolsDeferred } from './mcp/octocode.js';
import { envFlag } from './shared/env.js';
import { octocodePrompt } from './prompt.js';
import type { AgentProfile } from './subagents/profiles.js';
import { AGENT_COLLABORATE_ENV, AGENT_ID_ENV, AGENT_SCRATCH_ENV, PARENT_ID_ENV } from './team/store.js';

/** Pi tools that `file` covers. */
const FILE_COVERS = ['edit', 'write'];

/**
 * Tools the user named on purpose: `--tools`/`-t` on the command line and plain names in the `defaultTools` setting
 * (`+name` adds to Pi's defaults and `-name` removes, so neither names edit/write deliberately unless written out).
 */
export function explicitTools(argv: readonly string[], defaultTools: readonly string[] | undefined): Set<string> {
  const names = new Set<string>();
  for (let i = 0; i < argv.length - 1; i++) {
    if (argv[i] === '--tools' || argv[i] === '-t') for (const name of argv[i + 1]!.split(',')) names.add(name.trim());
  }
  for (const entry of defaultTools ?? []) names.add(entry.replace(/^\+/, '').trim());
  return names;
}

/**
 * The active set with `file` in place of Pi's edit/write, or undefined when nothing changes: `file` must itself be
 * active (an allowlist may leave it out, and the agent never loses editing), and edit/write the user enabled on purpose
 * stay.
 */
export function withFileTool(active: readonly string[], explicit: ReadonlySet<string>): string[] | undefined {
  if (!active.includes('file')) return undefined;
  const drop = FILE_COVERS.filter((name) => active.includes(name) && !explicit.has(name));
  return drop.length > 0 ? active.filter((name) => !drop.includes(name)) : undefined;
}

function identityFromEnv(env: NodeJS.ProcessEnv) {
  const id = env[AGENT_ID_ENV];
  if (!id) return undefined;
  return {
    id,
    ...(env[PARENT_ID_ENV] ? { parentId: env[PARENT_ID_ENV] } : {}),
    ...(envFlag(env, AGENT_COLLABORATE_ENV) ? { collaborate: true } : {}),
    ...(env[AGENT_SCRATCH_ENV] ? { scratch: env[AGENT_SCRATCH_ENV] } : {}),
  };
}

/**
 * At session start, `file` takes the place of Pi's edit/write (once; later tool changes are the user's). Each turn
 * adds the `octocode` prompt section, rebuilt only when its inputs change so the provider cache holds. The active
 * tool set is Pi's: it is never rewritten per turn.
 */
export function registerTurnSetup(pi: ExtensionAPI, options: { isSubagent: boolean; profiles: () => Map<string, AgentProfile> }): { reset(): void } {
  let prompt: string | undefined;
  let promptKey: string | undefined;
  /** Once Octocode is known to be here it stays, so the section does not flip (and break the cache) mid-session. */
  let octocodeSeen = false;

  pi.on('session_start', async () => {
    const next = withFileTool(pi.getActiveTools(), explicitTools(process.argv, pi.getSettings().defaultTools));
    if (next) pi.setActiveTools(next);
  });

  pi.on('before_agent_start', async (event, ctx) => {
    const selected = event.systemPromptOptions.selectedTools ?? pi.getActiveTools();
    // Pi snapshots selectedTools before this hook runs, and its MCP host connects servers inside the same hook, so on
    // the first prompt (the only one of most subagents) Octocode's tools are not there yet. Decide from the
    // registration instead (an `octocode` entry in Pi's mcp.json replaces it), or from the tools once they appear.
    const configured = configuredOctocodeServer(ctx.cwd ?? process.cwd(), ctx.isProjectTrusted?.() ?? false);
    const builtIn = !configured && octocodeMcpEnabled();
    octocodeSeen ||= builtIn || (configured !== undefined && configured.enabled !== false) || selected.some(isOctocodeTool) || pi.getAllTools().some((tool) => isOctocodeTool(tool.name));
    const octocode = octocodeSeen;
    const canWrite = ['file', 'write', 'edit'].some((name) => selected.includes(name));
    const identity = identityFromEnv(process.env);
    const inputs = {
      octocode,
      profiles: [...options.profiles().values()].map(({ name, description }) => ({ name, description })),
      canDelegate: !options.isSubagent,
      canUseAgent: selected.includes('agent'),
      canWrite,
      octocodeDeferred: builtIn && octocodeToolsDeferred(),
      ...(identity ? { identity } : {}),
    };
    const key = JSON.stringify(inputs);
    if (prompt === undefined || key !== promptKey) {
      promptKey = key;
      prompt = octocodePrompt(inputs);
    }
    // A named section (not a whole-prompt override) lets Pi record a small transcript delta.
    event.systemPromptOptions.sections['octocode'] = prompt;
    return undefined;
  });

  return {
    reset() {
      prompt = undefined;
      promptKey = undefined;
    },
  };
}

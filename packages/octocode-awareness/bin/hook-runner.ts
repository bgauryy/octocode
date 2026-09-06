export type { ShellHookHost } from './hook-payload.js';
export type { HookRunOptions } from './hook-payload.js';
export type { HookControlOutcome } from './hook-payload.js';
export { hookContextEnvelope } from './hook-payload.js';
export { hookBlockOutcome } from './hook-payload.js';
export { hookCommandForHostEvent } from './hook-payload.js';
import { HookRunOptions, INTERNAL_HOOK_HOST, INTERNAL_SKILL_ROOT, hookEventName, normalizeShellHookHost, parsePayload, readStdin, shellHookHost, workspace } from './hook-payload.js';
import { runPostEdit, runPreEdit } from './hook-edit-events.js';
import { runNotifyDeliver, runSessionCompact, runSessionEnd, runStopVerify } from './hook-lifecycle.js';
import { recordHookReceiptBestEffort } from '../src/hook-receipts.js';
import { AwarenessFeatureConfig, DEFAULT_AWARENESS_CONFIG, loadAwarenessConfig } from '../src/awareness-config.js';
import {
  hookCommandEnabled,
  loadWorkspacePolicy,
  type AwarenessHookProfile,
} from '../src/workspace-policy.js';
import { hookStateUnchanged, recordHookChangeState } from './hook-change-state.js';

function hookFeatures(): AwarenessFeatureConfig {
  try {
    const loaded = loadAwarenessConfig();
    if (!loaded.exists) return { ...loaded.config.features, hooks: false };
    return {
      ...loaded.config.features,
    };
  } catch (error) {
    console.error(`octocode-awareness config warning (hooks inert): ${(error as Error).message}`);
    return { ...DEFAULT_AWARENESS_CONFIG.features, hooks: false };
  }
}

export async function runHookCommand(
  command: string,
  rawPayload?: string,
  options: HookRunOptions = {},
): Promise<number> {
  if (command === 'help' || command === '--help' || command === '-h') {
    process.stdout.write('usage: hook-runner <pre-edit|post-edit|stop-verify|notify-deliver|session-compact|session-end> < hook-payload.json\n');
    return 0;
  }

  const knownCommands = new Set([
    'pre-edit', 'post-edit', 'stop-verify', 'notify-deliver', 'session-compact', 'session-end',
  ]);
  if (!knownCommands.has(command)) {
    console.error(`unknown hook command: ${command}`);
    return 1;
  }

  const features = hookFeatures();
  if (!features.hooks) return 0;

  const payload = {
    ...parsePayload(rawPayload ?? await readStdin()),
    ...(options.host ? { [INTERNAL_HOOK_HOST]: options.host } : {}),
    ...(options.skillRoot ? { [INTERNAL_SKILL_ROOT]: options.skillRoot } : {}),
  };
  const configuredProfile = process.env.OCTOCODE_HOOK_PROFILE
    ?? loadWorkspacePolicy(workspace(payload) ?? process.cwd()).policy.hooks.profile;
  if (!['guard', 'coordination', 'full'].includes(configuredProfile)) {
    console.error(`octocode-awareness hook profile warning (hooks inert): expected guard, coordination, or full; got ${configuredProfile}`);
    return 0;
  }
  const profile = configuredProfile as AwarenessHookProfile;
  if (!hookCommandEnabled(profile, command)) return 0;
  const receipt = (status: 'success' | 'failure') => recordHookReceiptBestEffort({
    workspacePath: workspace(payload) ?? process.cwd(),
    host: shellHookHost(payload),
    event: hookEventName(payload) ?? command,
    status,
  });
  if (command === 'notify-deliver' && hookStateUnchanged(payload)) {
    receipt('success');
    recordHookChangeState(payload);
    return 0;
  }
  try {
    let exitCode: number;
    switch (command) {
      case 'pre-edit': exitCode = await runPreEdit(payload, { emitPeerSignal: profile !== 'guard' }); break;
      case 'post-edit': exitCode = await runPostEdit(payload); break;
      case 'stop-verify': exitCode = await runStopVerify(payload, features); break;
      case 'notify-deliver': exitCode = await runNotifyDeliver(payload, features); break;
      case 'session-compact': exitCode = await runSessionCompact(payload, features); break;
      case 'session-end': exitCode = await runSessionEnd(payload, features); break;
      default: return 1;
    }
    receipt(exitCode === 1 ? 'failure' : 'success');
    if (command === 'notify-deliver') recordHookChangeState(payload);
    return exitCode;
  } catch (error) {
    receipt('failure');
    throw error;
  }
}

export async function main(): Promise<number> {
  const hostIndex = process.argv.indexOf('--host');
  const rawHost = hostIndex >= 0 ? process.argv[hostIndex + 1] : undefined;
  const host = normalizeShellHookHost(rawHost);
  if (rawHost && !host) {
    console.error(`unknown hook host: ${rawHost}`);
    return 1;
  }
  const skillRootIndex = process.argv.indexOf('--skill-root');
  const skillRoot = skillRootIndex >= 0 ? process.argv[skillRootIndex + 1] : undefined;
  return runHookCommand(process.argv[2] ?? 'help', undefined, {
    ...(host ? { host } : {}),
    ...(skillRoot ? { skillRoot } : {}),
  });
}

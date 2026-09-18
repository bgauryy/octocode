import { writeCommandDiagnostic, writeCommandText } from '../command-output.js';
import { HookRunOptions, INTERNAL_HOOK_HOST, INTERNAL_SKILL_ROOT, agentId, hookEventName, parsePayload, shellHookHost, workspace } from './payload.js';
import { runPostEdit, runPreEdit } from './edit-events.js';
import { canDeliverHookCommunication, isRetentionPreviewDue, runNotifyDeliver, runSessionCompact, runSessionEnd, runStopVerify, runToolCommunication } from './lifecycle.js';
import { normalizeToolHookPayload } from './tool-protocol.js';
import { captureHookHistory } from './history-capture.js';
import { HOOK_RECEIPT_SUCCESS_SAMPLE_MS, recordHookReceiptBestEffort } from '../hook-receipts.js';
import { AwarenessFeatureConfig, DEFAULT_AWARENESS_CONFIG, loadAwarenessConfig } from '../awareness-config.js';
import {
  hookCommandEnabled,
  loadWorkspacePolicy,
  type AwarenessHookProfile,
} from '../workspace-policy.js';
import { hookStateUnchanged, recordHookChangeState } from './change-state.js';

function hookFeatures(): AwarenessFeatureConfig {
  try {
    const loaded = loadAwarenessConfig();
    return {
      ...loaded.config.features,
    };
  } catch (error) {
    writeCommandDiagnostic(`octocode-awareness config warning (hooks inert): ${(error as Error).message}`);
    return { ...DEFAULT_AWARENESS_CONFIG.features, hooks: false };
  }
}

export async function runHookCommand(
  command: string,
  rawPayload: string,
  options: HookRunOptions = {},
): Promise<number> {
  if (command === 'help' || command === '--help' || command === '-h') {
    writeCommandText('usage: hook-runner <pre-edit|post-edit|stop-verify|notify-deliver|session-compact|session-end> < hook-payload.json\n');
    return 0;
  }

  const knownCommands = new Set([
    'pre-edit', 'post-edit', 'stop-verify', 'notify-deliver', 'session-compact', 'session-end',
  ]);
  if (!knownCommands.has(command)) {
    writeCommandDiagnostic(`unknown hook command: ${command}`);
    return 1;
  }

  let payload: Record<string, unknown> = {
    ...parsePayload(rawPayload),
    ...(options.host ? { [INTERNAL_HOOK_HOST]: options.host } : {}),
    ...(options.skillRoot ? { [INTERNAL_SKILL_ROOT]: options.skillRoot } : {}),
  };
  let configuredProfile: string | undefined;
  try {
    const workspacePath = workspace(payload) ?? process.cwd();
    const host = shellHookHost(payload);
    const policy = loadWorkspacePolicy(workspacePath).policy;
    if (policy.hooks.owners[host] === 'native') return 0;
    configuredProfile = process.env.OCTOCODE_HOOK_PROFILE
      ?? policy.hooks.profile;
  } catch (error) {
    writeCommandDiagnostic(`octocode-awareness hook policy warning (hooks inert): ${(error as Error).message}`);
    return 0;
  }
  const features = hookFeatures();
  if (!features.hooks) return 0;
  if (!['guard', 'coordination', 'full'].includes(configuredProfile)) {
    writeCommandDiagnostic(`octocode-awareness hook profile warning (hooks inert): expected guard, coordination, or full; got ${configuredProfile}`);
    return 0;
  }
  const profile = configuredProfile as AwarenessHookProfile;
  if (!hookCommandEnabled(profile, command)) return 0;
  let normalizedTool: ReturnType<typeof normalizeToolHookPayload> | null = null;
  if (profile !== 'coordination' && (command === 'pre-edit' || command === 'post-edit')) {
    if (!hookEventName(payload)) {
      const host = shellHookHost(payload);
      const inferredEvent = host === 'cursor'
        ? command === 'pre-edit' ? 'preToolUse' : 'postToolUse'
        : host === 'gemini'
          ? command === 'pre-edit' ? 'BeforeTool' : 'AfterTool'
          : host === 'opencode'
            ? command === 'pre-edit' ? 'tool.execute.before' : 'tool.execute.after'
            : command === 'pre-edit' ? 'PreToolUse' : 'PostToolUse';
      payload = { ...payload, hook_event_name: inferredEvent };
    }
    try {
      normalizedTool = normalizeToolHookPayload(payload, shellHookHost(payload));
    } catch (error) {
      writeCommandDiagnostic(`octocode-awareness hook payload warning (continuing): ${(error as Error).message}`);
      return 0;
    }
    if (normalizedTool.phase !== (command === 'pre-edit' ? 'pre' : 'post')) {
      writeCommandDiagnostic(`octocode-awareness hook payload warning (continuing): ${command} received ${normalizedTool.phase} event`);
      return 0;
    }
  }
  try {
    agentId(payload);
  } catch (error) {
    if (normalizedTool && normalizedTool.tool.effect !== 'workspace-write') return 0;
    writeCommandDiagnostic(`octocode-awareness hook identity error: ${(error as Error).message}`);
    return 1;
  }
  const receipt = (status: 'success' | 'degraded' | 'failure') => recordHookReceiptBestEffort({
    workspacePath: workspace(payload) ?? process.cwd(),
    host: shellHookHost(payload),
    event: hookEventName(payload) ?? command,
    status,
    ...(status === 'success' ? { minimumIntervalMs: HOOK_RECEIPT_SUCCESS_SAMPLE_MS } : {}),
  });
  const communicationOnly = command === 'notify-deliver' || (profile === 'coordination' && command === 'post-edit');
  // Unsupported context channels must not consume a message fingerprint or the
  // shared change token. A later supported boundary still needs to deliver it.
  if (communicationOnly && !canDeliverHookCommunication(payload)) {
    receipt('success');
    return 0;
  }
  if (communicationOnly && hookStateUnchanged(payload) && !isRetentionPreviewDue(payload, features)) {
    receipt('success');
    recordHookChangeState(payload);
    return 0;
  }
  try {
    let exitCode: number;
    switch (command) {
      case 'pre-edit': {
        if (normalizedTool?.tool.effect !== 'workspace-write') {
          exitCode = profile === 'guard' ? 0 : await runToolCommunication(payload, features);
        } else {
          exitCode = await runPreEdit(payload, { emitPeerSignal: profile !== 'guard' });
          if (exitCode === 0 && profile === 'full') await captureHookHistory(payload, normalizedTool);
        }
        break;
      }
      case 'post-edit': {
        if (profile === 'coordination') {
          exitCode = await runToolCommunication(payload, features);
          break;
        }
        if (normalizedTool?.tool.effect === 'workspace-write' && normalizedTool.outcome.terminal) {
          exitCode = await runPostEdit(payload);
          if (profile === 'full') await captureHookHistory(payload, normalizedTool);
        }
        else exitCode = 0;
        if (exitCode === 0 && profile !== 'guard') exitCode = await runToolCommunication(payload, features);
        break;
      }
      case 'stop-verify': exitCode = await runStopVerify(payload, features); break;
      case 'notify-deliver': exitCode = await runNotifyDeliver(payload, features); break;
      case 'session-compact': exitCode = await runSessionCompact(payload, features); break;
      case 'session-end': exitCode = await runSessionEnd(payload, features, { settleWork: profile !== 'coordination' }); break;
      default: return 1;
    }
    receipt(exitCode === 1 ? 'failure' : 'success');
    if (communicationOnly) recordHookChangeState(payload);
    return exitCode;
  } catch (error) {
    receipt('degraded');
    writeCommandDiagnostic(`octocode-awareness ${command} warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
    return 0;
  }
}

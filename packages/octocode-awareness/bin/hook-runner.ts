export type { ShellHookHost } from './hook-payload.js';
export type { HookRunOptions } from './hook-payload.js';
export type { HookControlOutcome } from './hook-payload.js';
export { hookContextEnvelope } from './hook-payload.js';
export { hookBlockOutcome } from './hook-payload.js';
import { HookRunOptions, INTERNAL_HOOK_HOST, INTERNAL_SKILL_ROOT, normalizeShellHookHost, parsePayload, readStdin } from './hook-payload.js';
import { runHarnessGuard, runPostEdit, runPreEdit } from './hook-edit-events.js';
import { runNotifyDeliver, runSessionCompact, runSessionEnd, runStopVerify } from './hook-lifecycle.js';

export async function runHookCommand(
  command: string,
  rawPayload?: string,
  options: HookRunOptions = {},
): Promise<number> {
  if (command === 'help' || command === '--help' || command === '-h') {
    process.stdout.write('usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-compact|session-end> < hook-payload.json\n');
    return 0;
  }

  const payload = {
    ...parsePayload(rawPayload ?? await readStdin()),
    ...(options.host ? { [INTERNAL_HOOK_HOST]: options.host } : {}),
    ...(options.skillRoot ? { [INTERNAL_SKILL_ROOT]: options.skillRoot } : {}),
  };
  switch (command) {
    case 'pre-edit': return runPreEdit(payload);
    case 'post-edit': return runPostEdit(payload);
    case 'harness-guard': return runHarnessGuard(payload);
    case 'stop-verify': return runStopVerify(payload);
    case 'notify-deliver': return runNotifyDeliver(payload);
    case 'session-compact': return runSessionCompact(payload);
    case 'session-end': return runSessionEnd(payload);
    default:
      console.error(`unknown hook command: ${command}`);
      return 1;
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

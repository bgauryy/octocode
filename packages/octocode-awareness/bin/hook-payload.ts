/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, file presence,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { createHash } from 'node:crypto';
import { basename, relative, resolve } from 'node:path';
import { connectDb, resolveDbPath } from '../src/db.js';
import { canonicalizePath } from '../src/git.js';
import { extractPiWriteTargetPaths } from '../src/pi-hooks.js';

export type ShellHookHost = 'claude' | 'codex' | 'cursor';

export interface HookRunOptions {
  host?: ShellHookHost;
  skillRoot?: string;
}

export interface HookControlOutcome {
  exitCode: number;
  payload?: Record<string, unknown>;
  stderr?: string;
}

export const INTERNAL_HOOK_HOST = '__octocode_hook_host';
export const INTERNAL_SKILL_ROOT = '__octocode_skill_root';

export function readStdin(): Promise<string> {
  return new Promise((resolve) => {
    let raw = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (chunk) => { raw += chunk; });
    process.stdin.on('end', () => resolve(raw));
    process.stdin.on('error', () => resolve(raw));
  });
}

export function parsePayload(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw || '{}');
    return parsed && typeof parsed === 'object' ? parsed as Record<string, unknown> : {};
  } catch {
    return raw.trim() ? { input: raw } : {};
  }
}

export function objectOrEmpty(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

export function payloadInput(payload: Record<string, unknown>): unknown {
  return payload.tool_input ?? payload.input ?? payload.args ?? payload;
}

export function payloadForFileExtraction(payload: Record<string, unknown>): unknown {
  const input = payloadInput(payload);
  const inputObj = objectOrEmpty(input);
  if (inputObj === payload) return input;
  if (Object.keys(inputObj).length === 0) return input;
  return { ...payload, ...inputObj };
}

export let warnedFallbackAgentId = false;

export function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return null;
}

export function normalizeShellHookHost(value: unknown): ShellHookHost | null {
  if (typeof value !== 'string') return null;
  const normalized = value.trim().toLowerCase();
  return normalized === 'claude' || normalized === 'codex' || normalized === 'cursor'
    ? normalized
    : null;
}

export function shellHookHost(payload: Record<string, unknown>): ShellHookHost {
  const explicit = normalizeShellHookHost(
    payload[INTERNAL_HOOK_HOST]
      ?? process.env.OCTOCODE_AGENT_HOST
      ?? payload.host
      ?? payload.client,
  );
  if (explicit) return explicit;
  const eventName = firstString(payload.hook_event_name, payload.eventName) ?? '';
  if (eventName && eventName[0] === eventName[0]?.toLowerCase()) return 'cursor';
  return 'claude';
}

export function hookSkillRoot(payload: Record<string, unknown>): string | null {
  return firstString(payload[INTERNAL_SKILL_ROOT], process.env.OCTOCODE_SKILL_ROOT);
}

export function hookContextEnvelope(
  host: ShellHookHost,
  eventName: string,
  message: string,
): Record<string, unknown> {
  if (host === 'cursor') {
    if (eventName === 'sessionStart') return { additional_context: message };
    return { permission: 'allow', agent_message: message };
  }
  return {
    hookSpecificOutput: {
      hookEventName: eventName,
      additionalContext: message,
    },
  };
}

export function hookBlockOutcome(
  host: ShellHookHost,
  phase: 'pre-edit' | 'stop',
  message: string,
): HookControlOutcome {
  if (host !== 'cursor') return { exitCode: 2, stderr: message };
  if (phase === 'stop') {
    return { exitCode: 0, payload: { followup_message: message } };
  }
  return {
    exitCode: 0,
    payload: {
      permission: 'deny',
      user_message: message,
      agent_message: message,
    },
  };
}

export function writeHookPayload(payload: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

export function emitHookContext(payload: Record<string, unknown>, eventName: string, message: string): void {
  writeHookPayload(hookContextEnvelope(shellHookHost(payload), eventName, message));
}

export function completeHookControl(outcome: HookControlOutcome): number {
  if (outcome.payload) writeHookPayload(outcome.payload);
  if (outcome.stderr) console.error(outcome.stderr);
  return outcome.exitCode;
}

export function agentId(payload: Record<string, unknown>): string {
  const input = objectOrEmpty(payloadInput(payload));
  const explicit = firstString(
    payload.agent_id,
    payload.agentId,
    input.agent_id,
    input.agentId,
    process.env.OCTOCODE_AGENT_ID,
    payload.session_id,
    payload.sessionId,
    input.session_id,
    input.sessionId,
  );
  if (explicit) return explicit;

  const host = firstString(
    payload[INTERNAL_HOOK_HOST],
    process.env.OCTOCODE_AGENT_HOST,
    payload.host,
    payload.client,
    payload.source,
    payload.context,
  ) ?? 'shell';
  const scope = `${host}\0${workspace(payload) ?? process.cwd()}`;
  const suffix = createHash('sha1').update(scope).digest('hex').slice(0, 12);
  const fallback = `hook:${host.replace(/[^a-zA-Z0-9_.:-]/g, '_')}:${suffix}`;
  if (!warnedFallbackAgentId) {
    warnedFallbackAgentId = true;
    console.error(`octocode-awareness: OCTOCODE_AGENT_ID or host session id missing; using fallback agent id "${fallback}". Set OCTOCODE_AGENT_ID for reliable multi-agent awareness.`);
  }
  return fallback;
}

export function sessionId(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    payload.session_id, payload.sessionId, input.session_id, input.sessionId,
  );
}

export function promptQuery(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  const prompt = firstString(
    payload.prompt,
    payload.user_prompt,
    payload.userPrompt,
    payload.text,
    payload.message,
    typeof payload.input === 'string' ? payload.input : null,
    input.prompt,
    input.user_prompt,
    input.userPrompt,
    input.text,
    input.message,
  );
  return prompt ? prompt.slice(0, 4_000) : null;
}

export function hookSessionCorrelation(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    sessionId(payload),
    payload.transcript_path,
    payload.transcriptPath,
    payload.conversation_id,
    payload.conversationId,
    payload.thread_id,
    payload.threadId,
    input.transcript_path,
    input.transcriptPath,
    input.conversation_id,
    input.conversationId,
    input.thread_id,
    input.threadId,
  );
}

export function toolName(payload: Record<string, unknown>): string {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    payload.tool_name, payload.toolName, payload.name, input.tool_name, input.toolName,
  ) ?? '';
}

// Build an informative auto-claim rationale from the tool + target files so a
// blocked agent sees WHAT the holder is doing, not a generic "file edit".
export function autoClaimRationale(payload: Record<string, unknown>, files: string[]): string {
  const tool = toolName(payload);
  const names = files.map((f) => f.split('/').pop() || f);
  const shown = names.slice(0, 3).join(', ');
  const extra = names.length > 3 ? ` +${names.length - 3} more` : '';
  const action = tool ? `${tool}` : 'edit';
  return `auto: ${action} ${shown}${extra} (lifecycle hook)`;
}

export function fallbackVerificationPlan(files: string[], cwd: string): string {
  const canonicalWorkspace = canonicalizePath(cwd);
  const normalized = [...new Set(files.map(file => resolveHookPath(file, cwd)))];
  const shown = normalized.slice(0, 3)
    .map(file => relative(canonicalWorkspace, file) || basename(file))
    .join(', ');
  const omitted = normalized.length > 3 ? ` (+${normalized.length - 3} more)` : '';
  return `Verify ${shown || 'the edited files'}${omitted}: run the smallest relevant test/typecheck and inspect the diff; record the check and result.`;
}

export function agentName(payload: Record<string, unknown>): string {
  const value =
    process.env.OCTOCODE_AGENT_NAME
    ?? payload.agent_name
    ?? payload.agentName
    ?? payload.agent_display_name
    ?? payload.agentDisplayName;
  return typeof value === 'string' && value.trim() ? value.trim() : '';
}

export function workspace(payload: Record<string, unknown>): string | null {
  const value = payload.cwd ?? payload.workspace ?? payload.workspacePath;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

export function artifact(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  const value =
    process.env.OCTOCODE_ARTIFACT
    ?? process.env.OCTOCODE_PACKAGE
    ?? process.env.OCTOCODE_SERVICE
    ?? payload.artifact
    ?? payload.package
    ?? payload.service
    ?? input.artifact
    ?? input.package
    ?? input.service;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

export function hookReason(payload: Record<string, unknown>): string {
  return typeof payload.reason === 'string' ? payload.reason : '';
}

export function isStopHookActive(payload: Record<string, unknown>): boolean {
  return Boolean(payload.stop_hook_active);
}

export function hookEventName(payload: Record<string, unknown>): string | null {
  return firstString(payload.hook_event_name, payload.eventName);
}

export function hookToolFailed(payload: Record<string, unknown>): boolean {
  const input = objectOrEmpty(payloadInput(payload));
  const response = objectOrEmpty(payload.tool_response ?? payload.toolResponse ?? payload.result);
  const event = hookEventName(payload)?.toLowerCase() ?? '';
  return event.includes('failure')
    || payload.is_error === true
    || payload.isError === true
    || input.is_error === true
    || input.isError === true
    || response.is_error === true
    || response.isError === true
    || response.success === false;
}

export function extractFiles(payload: Record<string, unknown>): string[] {
  const input = payloadForFileExtraction(payload);
  const inputObj = objectOrEmpty(input);
  const toolName = payload.tool_name ?? payload.toolName ?? payload.name ?? inputObj.tool_name ?? inputObj.toolName ?? '';
  return extractPiWriteTargetPaths(toolName, input, { assumeWrite: true });
}

export function resolveHookPath(file: string, cwd = process.cwd()): string {
  // Absolutize AND normalize: `..`/`.` segments and non-absolute inputs (Codex
  // apply_patch and Cursor payloads often carry repo-relative paths) must be
  // collapsed before any containment check, or a traversal path that actually
  // resolves inside the skill root can slip past a textual prefix comparison.
  return canonicalizePath(resolve(cwd, file));
}

export function db() {
  return connectDb(resolveDbPath(null));
}

import { afterEach, describe, expect, it } from 'vitest';
import {
  agentHost,
  agentName,
  agentVendor,
  artifact,
  autoClaimRationale,
  fallbackVerificationPlan,
  firstString,
  hookContextEnvelope,
  hookEventName,
  hookReason,
  hookSessionCorrelation,
  hookSkillRoot,
  hookToolFailed,
  isStopHookActive,
  objectOrEmpty,
  parsePayload,
  payloadForFileExtraction,
  payloadInput,
  promptQuery,
  sessionId,
  shellHookHost,
  toolName,
  workspace,
} from '../src/hooks/payload.js';

const originalHost = process.env.OCTOCODE_AGENT_HOST;
const originalSkillRoot = process.env.OCTOCODE_SKILL_ROOT;
const originalName = process.env.OCTOCODE_AGENT_NAME;
const originalVendor = process.env.OCTOCODE_AGENT_VENDOR;
const originalArtifact = process.env.OCTOCODE_ARTIFACT;
const originalPackage = process.env.OCTOCODE_PACKAGE;
const originalService = process.env.OCTOCODE_SERVICE;

afterEach(() => {
  for (const [key, value] of [
    ['OCTOCODE_AGENT_HOST', originalHost],
    ['OCTOCODE_SKILL_ROOT', originalSkillRoot],
    ['OCTOCODE_AGENT_NAME', originalName],
    ['OCTOCODE_AGENT_VENDOR', originalVendor],
    ['OCTOCODE_ARTIFACT', originalArtifact],
    ['OCTOCODE_PACKAGE', originalPackage],
    ['OCTOCODE_SERVICE', originalService],
  ] as const) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

describe('hook payload boundary normalization', () => {
  it('preserves useful raw and nested input shapes without inventing structure', () => {
    expect(parsePayload('')).toEqual({});
    expect(parsePayload('null')).toEqual({});
    expect(parsePayload('"scalar"')).toEqual({});
    expect(parsePayload('not-json')).toEqual({ input: 'not-json' });
    expect(parsePayload('   {broken')).toEqual({ input: '   {broken' });
    expect(objectOrEmpty(null)).toEqual({});
    expect(objectOrEmpty(['kept'])).toEqual(['kept']);

    expect(payloadInput({ input: 'plain' })).toBe('plain');
    expect(payloadInput({ tool_input: '{"path":"src/a.ts"}' })).toEqual({ path: 'src/a.ts' });
    expect(payloadInput({ toolArgs: '1' })).toBe('1');
    expect(payloadInput({ tool_input: '{bad' })).toBe('{bad');
    expect(payloadForFileExtraction({ marker: true })).toEqual({ marker: true });
    expect(payloadForFileExtraction({ tool_input: 'raw' })).toBe('raw');
    expect(payloadForFileExtraction({ tool_name: 'Write', tool_input: { path: 'src/a.ts' } }))
      .toMatchObject({ tool_name: 'Write', path: 'src/a.ts' });
    expect(firstString(null, ' ', 1)).toBeNull();
  });

  it('resolves host, identity metadata, workspace, and artifact precedence from explicit facts', () => {
    process.env.OCTOCODE_AGENT_HOST = 'gemini-cli';
    process.env.OCTOCODE_SKILL_ROOT = '/env/skill';
    process.env.OCTOCODE_AGENT_NAME = 'Env Name';
    process.env.OCTOCODE_AGENT_VENDOR = 'env-vendor';
    process.env.OCTOCODE_PACKAGE = 'env-package';
    expect(shellHookHost({})).toBe('gemini');
    expect(shellHookHost({ host: 'cursor' })).toBe('gemini');
    delete process.env.OCTOCODE_AGENT_HOST;
    expect(shellHookHost({ eventName: 'preToolUse' })).toBe('cursor');
    expect(shellHookHost({ eventName: 'PreToolUse' })).toBe('claude');
    expect(hookSkillRoot({})).toBe('/env/skill');
    expect(hookSkillRoot({ __octocode_skill_root: '/payload/skill' })).toBe('/payload/skill');
    expect(agentName({})).toBe('Env Name');
    expect(agentName({ agent_display_name: 'Payload Name' })).toBe('Payload Name');
    expect(agentHost({ client: 'codex' })).toBe('codex');
    expect(agentHost({})).toBeNull();
    expect(agentVendor({ model: { provider: 'child-provider' } })).toBe('child-provider');
    expect(agentVendor({})).toBe('env-vendor');
    expect(workspace({ workspacePath: ' /repo ' })).toBe('/repo');
    expect(workspace({ workspace: 42 })).toBeNull();
    expect(artifact({ service: 'payload-service' })).toBe('env-package');
    delete process.env.OCTOCODE_PACKAGE;
    expect(artifact({ tool_input: { service: 'nested-service' } })).toBe('nested-service');
    expect(artifact({ artifact: 42 })).toBeNull();
  });

  it('derives lifecycle correlation, prompts, rationale, and verification plans at their bounds', () => {
    expect(sessionId({ tool_input: { sessionId: 'nested-session' } })).toBe('nested-session');
    expect(sessionId({})).toBeNull();
    expect(promptQuery({ input: { message: 'nested prompt' } })).toBe('nested prompt');
    expect(promptQuery({ prompt: `  ${'x'.repeat(4_100)}  ` })).toHaveLength(4_000);
    expect(promptQuery({})).toBeNull();
    expect(hookSessionCorrelation({ input: { threadId: 'nested-thread' } })).toBe('nested-thread');
    expect(hookSessionCorrelation({ transcript_path: '/tmp/transcript' })).toBe('/tmp/transcript');
    expect(hookSessionCorrelation({})).toBeNull();
    expect(toolName({ name: 'apply_patch' })).toBe('apply_patch');
    expect(toolName({ input: { toolName: 'Write' } })).toBe('Write');
    expect(toolName({})).toBe('');
    expect(autoClaimRationale({}, [])).toBe('auto: edit  (lifecycle hook)');
    expect(autoClaimRationale({ tool_name: 'Edit' }, ['a.ts', 'b.ts', 'c.ts', 'd.ts']))
      .toBe('auto: Edit a.ts, b.ts, c.ts +1 more (lifecycle hook)');
    expect(fallbackVerificationPlan([], '/repo')).toContain('Verify the edited files');
    expect(fallbackVerificationPlan(['a.ts', 'b.ts', 'c.ts', 'd.ts'], '/repo'))
      .toContain('a.ts, b.ts, c.ts (+1 more)');
  });

  it('recognizes every generic terminal failure channel after protocol fallback', () => {
    expect(hookToolFailed({ hook_event_name: 'NotAToolEvent' })).toBe(false);
    for (const payload of [
      { is_error: true },
      { isError: true },
      { input: { is_error: true } },
      { input: { isError: true } },
      { tool_response: { is_error: true } },
      { toolResponse: { isError: true } },
      { result: { success: false } },
      { hook_event_name: 'lifecycleFailure' },
    ]) expect(hookToolFailed(payload)).toBe(true);
    expect(hookToolFailed({
      hook_event_name: 'PostToolUse', tool_name: 'Write', tool_input: { path: 'a.ts' },
    })).toBe(false);
    expect(hookReason({ reason: 'complete' })).toBe('complete');
    expect(hookReason({ reason: 42 })).toBe('');
    expect(isStopHookActive({ stop_hook_active: 1 })).toBe(true);
    expect(isStopHookActive({})).toBe(false);
    expect(hookEventName({ eventName: 'sessionEnd' })).toBe('sessionEnd');
    expect(hookEventName({})).toBeNull();
    expect(hookContextEnvelope('cursor', 'postToolUseFailure', 'not delivered')).toEqual({});
    expect(hookContextEnvelope('cursor', 'otherEvent', 'context')).toEqual({
      permission: 'allow', agent_message: 'context',
    });
  });
});

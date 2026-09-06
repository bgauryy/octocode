import { describe, expect, it } from 'vitest';
import {
  hookBlockOutcome,
  hookCommandForHostEvent,
  hookContextEnvelope,
} from '../bin/hook-runner.js';
import {
  normalizeShellHookHost,
  payloadInput,
  toolName,
} from '../bin/hook-payload.js';

describe('Copilot and Gemini hook payload adapters', () => {
  it('normalizes canonical host names and their CLI aliases', () => {
    expect(normalizeShellHookHost('copilot')).toBe('copilot');
    expect(normalizeShellHookHost('GitHub-Copilot')).toBe('copilot');
    expect(normalizeShellHookHost('gemini')).toBe('gemini');
    expect(normalizeShellHookHost('gemini-cli')).toBe('gemini');
    expect(normalizeShellHookHost('opencode')).toBe('opencode');
  });

  it('continues to reject unknown hook hosts', () => {
    expect(normalizeShellHookHost('unknown-agent')).toBeNull();
    expect(normalizeShellHookHost(undefined)).toBeNull();
  });

  it('accepts Copilot camelCase and PascalCase-event snake_case tool payloads', () => {
    const camelCase = {
      eventName: 'preToolUse',
      toolName: 'edit',
      toolArgs: { file_path: 'src/camel.ts' },
    };
    const snakeCase = {
      hook_event_name: 'PreToolUse',
      tool_name: 'Edit',
      tool_input: { file_path: 'src/snake.ts' },
    };

    expect(payloadInput(camelCase)).toEqual({ file_path: 'src/camel.ts' });
    expect(payloadInput(snakeCase)).toEqual({ file_path: 'src/snake.ts' });
    expect(toolName(camelCase)).toBe('edit');
    expect(toolName(snakeCase)).toBe('Edit');
  });

  it('uses Copilot permission decisions for tools and decision/reason for stop', () => {
    expect(hookContextEnvelope('copilot', 'preToolUse', 'peer changed')).toEqual({
      permissionDecision: 'allow',
      additionalContext: 'peer changed',
    });
    expect(hookBlockOutcome('copilot', 'pre-edit', 'exclusive conflict')).toEqual({
      exitCode: 0,
      payload: {
        permissionDecision: 'deny',
        permissionDecisionReason: 'exclusive conflict',
      },
    });
    expect(hookBlockOutcome('copilot', 'stop', 'verification debt')).toEqual({
      exitCode: 0,
      payload: { decision: 'block', reason: 'verification debt' },
    });
  });

  it('maps Copilot and Gemini events onto package-owned runner commands', () => {
    expect(hookCommandForHostEvent('copilot', 'preToolUse')).toBe('pre-edit');
    expect(hookCommandForHostEvent('copilot', 'PreToolUse')).toBe('pre-edit');
    expect(hookCommandForHostEvent('copilot', 'agentStop')).toBe('stop-verify');

    expect(hookCommandForHostEvent('gemini', 'BeforeTool')).toBe('pre-edit');
    expect(hookCommandForHostEvent('gemini', 'AfterTool')).toBe('post-edit');
    expect(hookCommandForHostEvent('gemini', 'AfterAgent')).toBe('stop-verify');
    expect(hookCommandForHostEvent('gemini', 'PreCompress')).toBe('session-compact');
    expect(hookCommandForHostEvent('gemini', 'SessionEnd')).toBe('session-end');
    expect(hookCommandForHostEvent('gemini', 'UnknownEvent')).toBeNull();
  });

  it('uses Gemini structured context and preserves exit-2 blocking', () => {
    expect(hookContextEnvelope('gemini', 'BeforeTool', 'peer changed')).toEqual({
      hookSpecificOutput: {
        hookEventName: 'BeforeTool',
        additionalContext: 'peer changed',
      },
    });
    expect(hookBlockOutcome('gemini', 'pre-edit', 'exclusive conflict')).toEqual({
      exitCode: 2,
      stderr: 'exclusive conflict',
    });
    expect(hookBlockOutcome('gemini', 'stop', 'verification debt')).toEqual({
      exitCode: 2,
      stderr: 'verification debt',
    });
  });

  it('keeps OpenCode on the generic plugin-translation contract', () => {
    expect(hookContextEnvelope('opencode', 'tool.execute.before', 'peer changed')).toEqual({
      hookSpecificOutput: {
        hookEventName: 'tool.execute.before',
        additionalContext: 'peer changed',
      },
    });
    expect(hookBlockOutcome('opencode', 'pre-edit', 'exclusive conflict')).toEqual({
      exitCode: 2,
      stderr: 'exclusive conflict',
    });
  });
});

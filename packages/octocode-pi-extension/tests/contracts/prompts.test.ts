import { describe, expect, it } from 'vitest';
import {
  PLAN_PROMPT_MAX_GOAL,
  PLAN_PROMPT_TRUNCATION_MARKER,
  SUBAGENT_PLACEHOLDERS,
  buildOctocodeSystemPrompt,
  buildPlanPrompt,
  expandSubagentPrompt,
} from '../../src/contracts/prompts/index.js';
import * as sharedPrompts from '../../src/contracts/prompts/index.js';

describe('shared prompts', () => {
  it('shares compact workflow, research, and continuity guidance', () => {
    const prompt = buildOctocodeSystemPrompt('');
    expect(prompt).toContain('<octocode_workflow>');
    expect(prompt).toContain('<octocode_research>');
    expect(prompt).toContain('<octocode_continuity>');
    expect(prompt).toContain('never imply approval');
    expect(prompt).toContain('continuations');
  });
  it('exports only prompts that participate in a supported runtime flow', () => {
    expect(sharedPrompts).not.toHaveProperty('MULTIDIMENSIONAL_MATHEMATICAL_FRAMEWORK_PROMPT');
  });

  it('composes the host coordination contract exactly once', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('<octocode_workflow>');
    expect(prompt.match(/<coordination>shared<\/coordination>/g)).toHaveLength(1);
    expect(prompt.endsWith('\n')).toBe(true);
  });

  it('does not hardcode a skill inventory into standing policy', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).not.toContain('octocode-eval-benchmark');
    expect(prompt).not.toContain('octocode-graph-eval');
  });

  it('defers tool inventory and call shape to advertised host contracts', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('active host tools through their advertised contracts');
    expect(prompt).not.toContain('chromeDebug');
    expect(prompt).not.toContain('localServer');
  });

  it('forbids automatic replay of crash-left effects whose outcome is unknown', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('Never automatically retry an effect left started or uncertain after a crash');
  });

  it('teaches efficient research routing without stale tool inventories', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain(sharedPrompts.LOCAL_TOOL_GUIDANCE);
    expect(prompt.match(/<octocode_research>/g)).toHaveLength(1);
    expect(sharedPrompts).toHaveProperty('LOCAL_TOOL_GUIDANCE');
    expect(prompt).not.toContain('localSearch operation:');
  });

  it('defers parameter contracts to live schemas rather than duplicating them in policy', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('Use an active target schema directly');
    expect(prompt).toContain('follow executable continuations');
    expect(prompt).not.toMatch(/fullContent|matchString|namePattern|pathRegex/);
  });

  it('keeps plan goals bounded and centralizes atomic Start semantics behind a host adapter', () => {
    const prompt = buildPlanPrompt('x'.repeat(PLAN_PROMPT_MAX_GOAL + 1), {
      proposalInstruction: 'Call the host plan envelope.',
      reviewInstruction: 'Show the host review card.',
    });
    expect(prompt).toContain(PLAN_PROMPT_TRUNCATION_MARKER);
    expect(prompt).toContain('Call the host plan envelope.');
    expect(prompt).toContain('Show the host review card.');
    expect(prompt).toContain('Start binds the exact displayed revision and begins the first runnable step in one action');
    expect(prompt).toContain('there is no separate Accept action');
    expect(prompt).toContain('a lightweight proposal omits the RFC path');
    expect(prompt).not.toContain('acceptance binds that revision but does not authorize implementation');
  });

  it('does not carry obsolete reflection-state policy', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).not.toContain('.octocode/REFLECT.md');
    expect(prompt).not.toContain('generated workspace state for reflection');
  });

  it('expands every shared subagent placeholder', () => {
    const source = SUBAGENT_PLACEHOLDERS.join('\n');
    const expanded = expandSubagentPrompt(source);
    for (const placeholder of SUBAGENT_PLACEHOLDERS) expect(expanded).not.toContain(placeholder);
  });

  it('selects worker-only constraints when the host supplies canonical Awareness guidance', () => {
    const prompt = expandSubagentPrompt(SUBAGENT_PLACEHOLDERS.join('\n'), { coordination: 'worker-only' });
    expect(prompt).toContain('The parent owns scope, synthesis, dependent decisions, and user contact');
    expect(prompt).toContain('Edit only explicitly owned paths or symbols');
    expect(prompt).toContain('wait for explicit release or reassignment');
    for (const marker of ['[DONE]', '[BLOCKED]', '[FAILED]', '[ARTIFACT]', '[EVIDENCE]', '[VERIFICATION]']) {
      expect(prompt).toContain(marker);
    }
    expect(prompt).not.toContain('Send new signals with signal publish');
    expect(prompt).not.toContain('signal ack');
    expect(prompt).not.toContain('You are auto-registered');
    expect(prompt).toContain('Never run any Git command unless the current user request explicitly asks');
    expect(expandSubagentPrompt('{{OCTOCODE_COORDINATION}}')).toContain('context.orient');
  });

  it('prefers native Awareness and limits CLI fallback without widening worker shell or Git authority', () => {
    const prompt = expandSubagentPrompt('{{OCTOCODE_SURFACE}}');
    expect(prompt).toContain('Use native Awareness for coordination');
    expect(prompt).toContain('only when unavailable, use the bound CLI');
    expect(prompt).toContain('with the supplied database, workspace, and stable identity');
    expect(prompt).toContain('Shell is limited to role-authorized tests, builds, and debugging');
    expect(prompt).toContain('coding, review, status, and verification alone do not authorize it');
    expect(prompt).toContain('Never run any Git command unless the current user request explicitly asks for Git');
    expect(prompt).toContain('including read-only inspection');
  });

  it('routes worker coordination through the five-concept client vocabulary', () => {
    const prompt = expandSubagentPrompt('{{OCTOCODE_COORDINATION}}');
    for (const text of ['context.orient', 'Work', 'message.send', 'message.reply', 'exact message ID', 'Memory', 'History']) {
      expect(prompt).toContain(text);
    }
    expect(prompt).not.toMatch(/signal publish|signal reply|signal ack|attend/i);
  });
});

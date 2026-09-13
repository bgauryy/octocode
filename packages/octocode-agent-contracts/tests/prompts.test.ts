import { describe, expect, it } from 'vitest';
import {
  PLAN_PROMPT_MAX_GOAL,
  PLAN_PROMPT_TRUNCATION_MARKER,
  SUBAGENT_PLACEHOLDERS,
  buildOctocodeSystemPrompt,
  buildPlanPrompt,
  expandSubagentPrompt,
} from '../src/prompts/index.js';
import * as sharedPrompts from '../src/prompts/index.js';

describe('shared prompts', () => {
  it('shares compact widget and context guidance without introducing host tool names', () => {
    const prompt = buildOctocodeSystemPrompt('');
    expect(prompt).toContain('<interaction_context>');
    expect(prompt).toContain('plain messages');
    expect(prompt).toContain('never imply approval');
    expect(prompt).toContain('continuations');
    expect(sharedPrompts).toHaveProperty('INTERACTION_CONTEXT_GUIDANCE');
  });
  it('exports only prompts that participate in a supported runtime flow', () => {
    expect(sharedPrompts).not.toHaveProperty('MULTIDIMENSIONAL_MATHEMATICAL_FRAMEWORK_PROMPT');
  });

  it('composes the host coordination contract exactly once', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('<authority>');
    expect(prompt.match(/<coordination>shared<\/coordination>/g)).toHaveLength(1);
    expect(prompt.endsWith('\n')).toBe(true);
  });

  it('routes measured improvement loops to the installed eval skill without hijacking ordinary retries', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('octocode-eval-benchmark');
    expect(prompt).toContain('ordinary retries use their direct acceptance check');
    expect(prompt).not.toContain('octocode-graph-eval');
  });

  it('keeps the shared policy host-neutral instead of advertising Pi-only tools', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    for (const piOnlyName of ['chromeDebug', 'browser agent', 'askUser', 'localServer']) {
      expect(prompt).not.toContain(piOnlyName);
    }
    expect(prompt).toContain('advertised host capabilities');
  });

  it('forbids replaying crash-left effects whose outcome is unknown', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('crash-left `started` effect');
    expect(prompt).toContain('terminal `uncertain`');
    expect(prompt).toContain('Never re-execute it');
  });

  it('uses the negotiated catalog and teaches efficient research routing without stale tool inventories', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('Reuse an observed schema');
    expect(prompt).toContain('A catalog selects a tool; its exact schema defines a valid call');
    expect(prompt).toContain('describe an unfamiliar contract once');
    expect(prompt).toContain(sharedPrompts.LOCAL_TOOL_GUIDANCE);
    expect(prompt.match(/<local_tools>/g)).toHaveLength(1);
    expect(sharedPrompts).toHaveProperty('LOCAL_TOOL_GUIDANCE');
    expect(prompt).not.toContain('localSearch operation:');
  });

  it('defers parameter contracts to live schemas rather than duplicating them in policy', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('live contract owns field names');
    expect(prompt).toContain('next.* continuations unchanged');
    expect(prompt).toContain('incomplete or unsupported searches cannot prove absence');
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

  it('does not instruct agents to mutate workspace reflection state', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('never hand-edit generated workspace state for reflection');
    expect(prompt).not.toContain('.octocode/REFLECT.md');
    expect(prompt).toContain('clickable path:line anchors');
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

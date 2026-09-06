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
  it('exports only prompts that participate in a supported runtime flow', () => {
    expect(sharedPrompts).not.toHaveProperty('MULTIDIMENSIONAL_MATHEMATICAL_FRAMEWORK_PROMPT');
  });

  it('composes the host coordination contract exactly once', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('<authority>');
    expect(prompt.match(/<coordination>shared<\/coordination>/g)).toHaveLength(1);
    expect(prompt.endsWith('\n')).toBe(true);
  });

  it('routes measurable loops to the installed graph-eval skill without legacy aliases', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('octocode-graph-eval');
    expect(prompt).not.toContain('octocode-eval');
  });

  it('keeps the shared policy host-neutral instead of advertising Pi-only tools', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    for (const piOnlyName of ['chromeDebug', 'browser agent', 'askUser', 'localServer']) {
      expect(prompt).not.toContain(piOnlyName);
    }
    expect(prompt).toContain('live host capability catalog');
  });

  it('forbids replaying crash-left effects whose outcome is unknown', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('crash-left `started` effect');
    expect(prompt).toContain('terminal `uncertain`');
    expect(prompt).toContain('Never re-execute it');
  });

  it('uses the negotiated research catalog instead of stale inner tool names', () => {
    const prompt = buildOctocodeSystemPrompt('<coordination>shared</coordination>');
    expect(prompt).toContain('Call catalog before choosing and schema before the first call');
    expect(prompt).toContain('never reuse an absent name');
    expect(prompt).not.toContain('localSearch operation:');
  });

  it('keeps plan goals bounded', () => {
    const prompt = buildPlanPrompt('x'.repeat(PLAN_PROMPT_MAX_GOAL + 1));
    expect(prompt).toContain(PLAN_PROMPT_TRUNCATION_MARKER);
  });

  it('expands every shared subagent placeholder', () => {
    const source = SUBAGENT_PLACEHOLDERS.join('\n');
    const expanded = expandSubagentPrompt(source);
    for (const placeholder of SUBAGENT_PLACEHOLDERS) expect(expanded).not.toContain(placeholder);
  });
});

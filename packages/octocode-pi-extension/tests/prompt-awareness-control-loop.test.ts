import { describe, expect, it } from 'vitest';
import { memory, skills, thinkFirst, workMode } from '../src/prompts/sections/index.js';

const loopPrompt = [thinkFirst, workMode, memory, skills].join('\n');

describe('Awareness prompt control loop', () => {
  it('routes authorized changes through ordered phases with conditional tail work', () => {
    const phases = [
      'BEFORE/REASON',
      'DURING/DO+COORDINATE',
      'AFTER/VERIFY',
      'LEARN?',
      'CLEAN?',
      'PROJECT?',
    ];
    let previous = -1;
    for (const phase of phases) {
      const current = workMode.indexOf(phase);
      expect(current, `${phase} is present`).toBeGreaterThan(previous);
      previous = current;
    }
    expect(workMode).toMatch(/answer\/review\/status → inspect and answer/);
    expect(workMode).toMatch(/diagnose → find cause only/);
    expect(workMode).toMatch(/plan-only → plan and stop/);
    expect(workMode).toMatch(/last three phases run only when their triggers/);
  });

  it('keeps recipes in the skill and makes persistence pressure-driven', () => {
    expect(skills).toMatch(/It owns plan\/task\/WORK.*recipes/);
    expect(memory).toMatch(/only for reusable, verified future value/);
    expect(memory).toMatch(/only when live reads measure pressure/);
    expect(memory).toMatch(/pending verification.*declared check.*verify mark.*verify audit/);
    expect(memory).toMatch(/stale memory.*octocode-memory-digest.*octocode-memory-forget/);
    expect(memory).toMatch(/stale locks\/signals.*lock prune.*signal prune.*--dry-run/);
    expect(memory).toMatch(/Dry-run cleanup first/);
    expect(memory).toMatch(/never clean after every task/);
    expect(memory).toMatch(/repo inject.*only when file readers need/);
    expect(memory).toMatch(/SQLite\/live queries remain canonical/);
    expect(memory).not.toMatch(/after work, run stale-memory/i);
    expect(memory).not.toContain('GOTCHAS.md');
    expect(skills).not.toMatch(/then verify, hand off, and clean stale state/);
  });

  it('preserves the operational command surface without prompt growth', () => {
    for (const command of [
      'attend', 'task ready', 'work start', 'work start --exclusive',
      'memory recall', 'refinement get', 'task submit', 'verify mark', 'verify audit',
      'memory record', 'reflect record', '/octocode-memory-digest',
      '/octocode-memory-forget', 'repo inject',
    ]) {
      expect(loopPrompt).toContain(command);
    }
    const lines = loopPrompt.split('\n').length;
    const words = loopPrompt.trim().split(/\s+/).length;
    expect(lines).toBeLessThanOrEqual(58);
    expect(words).toBeLessThanOrEqual(811);
  });
});

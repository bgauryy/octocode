import { describe, expect, it } from 'vitest';
import { awareness, skills, thinkFirst, workMode } from '../src/prompts/sections/index.js';

const loopPrompt = [thinkFirst, workMode, awareness, skills].join('\n');

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
    expect(awareness).toMatch(/skill owns routing and the CLI owns live state/);
    expect(awareness).toMatch(/memory recall/);
    expect(awareness).toMatch(/maintenance digest/);
    expect(awareness).toMatch(/wiki sync.*only when file readers need/);
    expect(awareness).toMatch(/SQLite and live CLI queries remain canonical/);
    expect(awareness).not.toMatch(/octocode-memory-(digest|forget)/);
    expect(skills).not.toMatch(/then verify, hand off, and clean stale state/);
  });

  it('preserves the operational command surface without prompt growth', () => {
    for (const command of [
      'attend', 'memory recall', 'memory record', 'task submit',
      'verify mark', 'verify audit', 'maintenance digest', 'memory forget',
      'wiki sync',
    ]) {
      expect(loopPrompt).toContain(command);
    }
    const lines = loopPrompt.split('\n').length;
    const words = loopPrompt.trim().split(/\s+/).length;
    expect(lines).toBeLessThanOrEqual(58);
    expect(words).toBeLessThanOrEqual(811);
  });
});

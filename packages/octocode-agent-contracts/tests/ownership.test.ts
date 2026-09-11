import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

function source(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

describe('shared-definition ownership', () => {
  it('keeps shared plan bounds in the contract package for the current Pi consumer', () => {
    const prompt = source('../../octocode-pi-extension/src/prompts/plan-prompt.ts');
    expect(prompt).toContain('@octocodeai/agent-contracts/prompts');
    expect(prompt).not.toMatch(/(?:const|let)\s+PLAN_PROMPT_(?:MAX_GOAL|TRUNCATION_MARKER)\s*=/);
  });

  it('keeps Awareness operating policy in its owner instead of copying it into Pi', () => {
    const prompt = source('../../octocode-pi-extension/src/prompts/system-prompt.ts');
    expect(prompt).toContain(
      "import { AWARENESS_PI_HOST_PROMPT } from '@octocodeai/octocode-awareness/host'",
    );
    expect(prompt).not.toContain('signal publish');
    expect(prompt).not.toContain('verify audit');
  });
});

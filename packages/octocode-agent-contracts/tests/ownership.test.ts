import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

function source(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

describe('shared-definition ownership', () => {
  it('keeps the native prompt module as an adapter instead of a policy copy', () => {
    const nativePrompt = source('../../octocode-agent/src/native-prompt.ts');
    expect(nativePrompt).not.toContain('const authority =');
    expect(nativePrompt).toContain('@octocodeai/agent-contracts/prompts');
  });

  it('keeps prompt-mode policy out of the native host adapter', () => {
    const nativePrompt = source('../../octocode-agent/src/native-prompt.ts');
    expect(nativePrompt).not.toContain("OCTOCODE_PROMPT_MODE = 'octocode-first'");
  });
});

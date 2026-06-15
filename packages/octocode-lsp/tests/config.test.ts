import { describe, expect, it } from 'vitest';

import { detectLanguageId, getLanguageServerForFile } from '../src/config.js';

describe('native config wrappers', () => {
  it('detects language ids and default native server configs', async () => {
    expect(detectLanguageId('demo.ts')).toBe('typescript');
    expect(detectLanguageId('demo.unknown')).toBe('plaintext');
    await expect(
      getLanguageServerForFile('demo.ts', process.cwd())
    ).resolves.toMatchObject({ languageId: 'typescript' });
  });
});

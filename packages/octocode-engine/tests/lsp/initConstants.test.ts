import { describe, expect, it } from 'vitest';
import { TSSERVER_LANGUAGE_IDS } from '../../src/lsp/initConstants.js';

describe('initConstants', () => {
  it('TSSERVER_LANGUAGE_IDS is a Set containing core TS/JS language ids', () => {
    expect(TSSERVER_LANGUAGE_IDS).toBeInstanceOf(Set);
    expect(TSSERVER_LANGUAGE_IDS.has('typescript')).toBe(true);
    expect(TSSERVER_LANGUAGE_IDS.has('typescriptreact')).toBe(true);
    expect(TSSERVER_LANGUAGE_IDS.has('javascript')).toBe(true);
    expect(TSSERVER_LANGUAGE_IDS.has('javascriptreact')).toBe(true);
  });
});

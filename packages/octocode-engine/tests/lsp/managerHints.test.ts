import { describe, expect, it } from 'vitest';

import {
  TOOLCHAIN_SERVERS,
  unavailableHintFor,
} from '../../src/lsp/manager.js';

describe('external language-server guidance', () => {
  it.each([
    ['php', 'intelephense'],
    ['ruby', 'ruby-lsp'],
    ['kotlin', 'kotlin-language-server'],
    ['elixir', 'elixir-ls'],
    ['sql', 'sqls'],
    ['scala', 'metals'],
  ])('provides an actionable %s installation hint', (languageId, server) => {
    expect(TOOLCHAIN_SERVERS).toContainEqual(
      expect.objectContaining({ languageId, server })
    );
    expect(unavailableHintFor(languageId, server)).toContain(server);
  });
});

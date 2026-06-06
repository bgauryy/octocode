import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SymbolResolver } from '../../src/lsp/resolver.js';

describe('SymbolResolver - isIdentifierChar optimization', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should resolve symbols efficiently without per-call regex compilation', () => {
    const resolver = new SymbolResolver();

    const lines = Array.from(
      { length: 100 },
      (_, i) => `const var${i} = mySymbol + otherVar;`
    );
    const content = lines.join('\n');

    const start = performance.now();
    for (let i = 0; i < 100; i++) {
      try {
        resolver.resolvePositionFromContent(content, {
          symbolName: 'mySymbol',
          lineHint: i + 1,
        });
      } catch {
        void 0;
      }
    }
    const elapsed = performance.now() - start;

    expect(elapsed).toBeLessThan(100);
  });

  it('should correctly identify identifier chars: letters, digits, _, $', () => {
    const resolver = new SymbolResolver();

    const content = `const $value = 1;`;
    const result = resolver.resolvePositionFromContent(content, {
      symbolName: '$value',
      lineHint: 1,
    });
    expect(result.position.character).toBe(6);

    const content2 = `const _private = 1;`;
    const result2 = resolver.resolvePositionFromContent(content2, {
      symbolName: '_private',
      lineHint: 1,
    });
    expect(result2.position.character).toBe(6);

    const content3 = `const _privateVar = _private;`;
    const result3 = resolver.resolvePositionFromContent(content3, {
      symbolName: '_private',
      lineHint: 1,
    });
    expect(result3.position.character).toBe(20);
  });

  it('should handle symbols at start and end of line', () => {
    const resolver = new SymbolResolver();

    const content = `myVar = 1;`;
    const result = resolver.resolvePositionFromContent(content, {
      symbolName: 'myVar',
      lineHint: 1,
    });
    expect(result.position.character).toBe(0);

    const content2 = `const x = myVar`;
    const result2 = resolver.resolvePositionFromContent(content2, {
      symbolName: 'myVar',
      lineHint: 1,
    });
    expect(result2.position.character).toBe(10);
  });
});

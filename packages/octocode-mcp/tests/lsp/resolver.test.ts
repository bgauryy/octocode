import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  SymbolResolver,
  SymbolResolutionError,
  defaultResolver,
} from '../../src/lsp/resolver.js';

describe('SymbolResolver', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('resolvePositionFromContent', () => {
    it('should find symbol on exact line', () => {
      const resolver = new SymbolResolver();
      const content = `function test() {
  const myVar = 1;
  return myVar;
}`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'myVar',
        lineHint: 2,
      });

      expect(result.position.line).toBe(1); // 0-indexed
      expect(result.position.character).toBe(8); // "  const "
      expect(result.foundAtLine).toBe(2);
      expect(result.lineOffset).toBe(0);
    });

    it('should find symbol on line above (within search radius)', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 2 });
      const content = `function test() {
  const target = 1;
  console.log("hi");
  return 0;
}`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'target',
        lineHint: 4, // Line 4, but symbol is on line 2
      });

      expect(result.foundAtLine).toBe(2);
      expect(result.lineOffset).toBe(-2);
    });

    it('should find symbol on line below (within search radius)', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 2 });
      const content = `function test() {
  console.log("hi");
  const target = 1;
  return 0;
}`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'target',
        lineHint: 1, // Line 1, but symbol is on line 3
      });

      expect(result.foundAtLine).toBe(3);
      expect(result.lineOffset).toBe(2);
    });

    it('should respect orderHint for multiple occurrences', () => {
      const resolver = new SymbolResolver();
      const content = `const x = x + x;`;

      // First occurrence
      const result0 = resolver.resolvePositionFromContent(content, {
        symbolName: 'x',
        lineHint: 1,
        orderHint: 0,
      });
      expect(result0.position.character).toBe(6); // "const "

      // Second occurrence
      const result1 = resolver.resolvePositionFromContent(content, {
        symbolName: 'x',
        lineHint: 1,
        orderHint: 1,
      });
      expect(result1.position.character).toBe(10); // "const x = "

      // Third occurrence
      const result2 = resolver.resolvePositionFromContent(content, {
        symbolName: 'x',
        lineHint: 1,
        orderHint: 2,
      });
      expect(result2.position.character).toBe(14); // "const x = x + "
    });

    it('should throw SymbolResolutionError for symbol not found', () => {
      const resolver = new SymbolResolver();
      const content = `function test() {
  return 1;
}`;

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'notFound',
          lineHint: 2,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should throw SymbolResolutionError for out-of-range line', () => {
      const resolver = new SymbolResolver();
      const content = `line1
line2`;

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'test',
          lineHint: 100,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should throw SymbolResolutionError for negative line', () => {
      const resolver = new SymbolResolver();
      const content = `line1
line2`;

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'test',
          lineHint: 0,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should respect word boundaries', () => {
      const resolver = new SymbolResolver();
      const content = `const fooBar = 1;
const foo = 2;`;

      // Should NOT match "foo" inside "fooBar"
      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'foo',
        lineHint: 1,
      });

      // Should find "foo" on line 2, not partial match in "fooBar"
      expect(result.foundAtLine).toBe(2);
    });

    it('should handle CRLF line endings', () => {
      const resolver = new SymbolResolver();
      const content = `function test() {\r\n  const myVar = 1;\r\n  return myVar;\r\n}`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'myVar',
        lineHint: 2,
      });

      expect(result.position.line).toBe(1);
      expect(result.foundAtLine).toBe(2);
    });

    it('should handle symbols at start of line', () => {
      const resolver = new SymbolResolver();
      const content = `myFunc()`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'myFunc',
        lineHint: 1,
      });

      expect(result.position.character).toBe(0);
    });

    it('should handle symbols at end of line', () => {
      const resolver = new SymbolResolver();
      const content = `const x = myFunc`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'myFunc',
        lineHint: 1,
      });

      expect(result.position.character).toBe(10);
    });

    it('should handle empty lines in content', () => {
      const resolver = new SymbolResolver();
      const content = `function test() {

  const myVar = 1;

  return myVar;
}`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'myVar',
        lineHint: 3,
      });

      expect(result.foundAtLine).toBe(3);
    });

    it('should handle underscore in identifiers', () => {
      const resolver = new SymbolResolver();
      const content = `const my_var = _privateVar + __dunder__;`;

      const result1 = resolver.resolvePositionFromContent(content, {
        symbolName: 'my_var',
        lineHint: 1,
      });
      expect(result1.position.character).toBe(6);

      const result2 = resolver.resolvePositionFromContent(content, {
        symbolName: '_privateVar',
        lineHint: 1,
      });
      expect(result2.position.character).toBe(15);
    });

    it('should handle $ in identifiers', () => {
      const resolver = new SymbolResolver();
      const content = `const $element = $$('selector');`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: '$element',
        lineHint: 1,
      });
      expect(result.position.character).toBe(6);
    });

    it('should search in correct order (exact first, then alternating)', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 3 });
      const content = `line1
targetAbove
line3
line4 (hint)
line5
targetBelow
line7`;

      // When searching from line 4, should check: 4, 3, 5, 2, 6, 1, 7
      // Should find "targetAbove" on line 2 before "targetBelow" on line 6
      const resultAbove = resolver.resolvePositionFromContent(content, {
        symbolName: 'targetAbove',
        lineHint: 4,
      });
      expect(resultAbove.foundAtLine).toBe(2);
      expect(resultAbove.lineOffset).toBe(-2);

      const resultBelow = resolver.resolvePositionFromContent(content, {
        symbolName: 'targetBelow',
        lineHint: 4,
      });
      expect(resultBelow.foundAtLine).toBe(6);
      expect(resultBelow.lineOffset).toBe(2);
    });
  });

  describe('resolvePosition (async)', () => {
    // Note: These tests require actual file access, so we test via synchronous methods
    // The async resolvePosition is a thin wrapper around resolvePositionFromContent

    it('should be an async method', () => {
      const resolver = new SymbolResolver();
      expect(typeof resolver.resolvePosition).toBe('function');
    });
  });

  describe('extractContext', () => {
    it('should extract context around a line', () => {
      const resolver = new SymbolResolver();
      const content = `line1
line2
line3
line4
line5
line6
line7`;

      const context = resolver.extractContext(content, 4, 2);

      expect(context.startLine).toBe(2);
      expect(context.endLine).toBe(6);
      expect(context.content).toBe('line2\nline3\nline4\nline5\nline6');
    });

    it('should handle context at start of file', () => {
      const resolver = new SymbolResolver();
      const content = `line1
line2
line3`;

      const context = resolver.extractContext(content, 1, 2);

      expect(context.startLine).toBe(1);
      expect(context.endLine).toBe(3);
    });

    it('should handle context at end of file', () => {
      const resolver = new SymbolResolver();
      const content = `line1
line2
line3`;

      const context = resolver.extractContext(content, 3, 2);

      expect(context.startLine).toBe(1);
      expect(context.endLine).toBe(3);
    });

    it('should handle zero context lines', () => {
      const resolver = new SymbolResolver();
      const content = `line1
line2
line3`;

      const context = resolver.extractContext(content, 2, 0);

      expect(context.startLine).toBe(2);
      expect(context.endLine).toBe(2);
      expect(context.content).toBe('line2');
    });
  });

  describe('SymbolResolutionError', () => {
    it('should contain all error properties', () => {
      const error = new SymbolResolutionError('mySymbol', 42, 'Test reason', 3);

      expect(error.name).toBe('SymbolResolutionError');
      expect(error.symbolName).toBe('mySymbol');
      expect(error.lineHint).toBe(42);
      expect(error.reason).toBe('Test reason');
      expect(error.searchRadius).toBe(3);
      expect(error.message).toContain('mySymbol');
      expect(error.message).toContain('42');
    });

    it('should use default search radius', () => {
      const error = new SymbolResolutionError('sym', 10, 'reason');
      expect(error.searchRadius).toBe(5);
    });
  });

  describe('defaultResolver', () => {
    it('should be a SymbolResolver instance', () => {
      expect(defaultResolver).toBeInstanceOf(SymbolResolver);
    });

    it('should have default lineSearchRadius of 5', () => {
      const content = `line1
line2
line3
target
line5`;

      // Should find "target" within radius 2 from line 2
      const result = defaultResolver.resolvePositionFromContent(content, {
        symbolName: 'target',
        lineHint: 2,
      });
      expect(result.foundAtLine).toBe(4);
    });
  });

  describe('resolveSymbolPosition', () => {
    // Note: resolveSymbolPosition is an async convenience function
    // that wraps the synchronous resolvePositionFromContent

    it('should be exported as a function', async () => {
      const { resolveSymbolPosition } =
        await import('../../src/lsp/resolver.js');
      expect(typeof resolveSymbolPosition).toBe('function');
    });
  });

  describe('edge cases', () => {
    it('should handle single-line file', () => {
      const resolver = new SymbolResolver();
      const content = 'const x = 1;';

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'x',
        lineHint: 1,
      });

      expect(result.foundAtLine).toBe(1);
    });

    it('should handle empty file', () => {
      const resolver = new SymbolResolver();
      const content = '';

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'x',
          lineHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should handle file with only empty lines', () => {
      const resolver = new SymbolResolver();
      const content = '\n\n\n';

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'x',
          lineHint: 2,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should handle symbol that looks like regex special char', () => {
      const resolver = new SymbolResolver();
      const content = 'const $test = 1;';

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: '$test',
        lineHint: 1,
      });

      expect(result.foundAtLine).toBe(1);
    });

    it('should not match symbol in middle of word', () => {
      const resolver = new SymbolResolver();
      const content = 'const testing = 1;';

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'test',
          lineHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should match symbol followed by punctuation', () => {
      const resolver = new SymbolResolver();
      const content = 'func();';

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'func',
        lineHint: 1,
      });

      expect(result.position.character).toBe(0);
    });

    it('should handle custom search radius', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 5 });
      const content = `line1
line2
line3
line4
line5
line6
target
line8`;

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'target',
        lineHint: 2, // 5 lines away
      });

      expect(result.foundAtLine).toBe(7);
    });
  });

  describe('orderHint on re-exported symbols (nearby lines)', () => {
    it('should find re-exported symbol on nearby line even with orderHint > 0', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 3 });
      const content = [
        'export {',
        '  SymbolResolver,',
        '  SymbolResolutionError,',
        '  defaultResolver,',
        "} from './resolver.js';",
      ].join('\n');

      // lineHint points to "export {" (line 1) but symbol is on line 2.
      // With orderHint: 1 the old code would demand a 2nd occurrence on
      // each nearby line and fail — the fix ignores orderHint for nearby lines.
      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'SymbolResolver',
        lineHint: 1,
        orderHint: 1,
      });

      expect(result.foundAtLine).toBe(2);
      expect(result.lineContent).toContain('SymbolResolver');
    });

    it('should still respect orderHint on the exact target line', () => {
      const resolver = new SymbolResolver();
      const content = 'const x = x + x;';

      // orderHint: 1 → second occurrence on the SAME line
      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'x',
        lineHint: 1,
        orderHint: 1,
      });

      expect(result.position.character).toBe(10); // "const x = " → second x
    });

    it('should find barrel re-export when lineHint is slightly off', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 3 });
      const content = [
        "export { acquirePooledClient } from './client.js';",
        '',
        "export { isLanguageServerAvailable } from './manager.js';",
      ].join('\n');

      // lineHint is 2 (empty line), orderHint non-zero.
      // Should still find isLanguageServerAvailable on line 3.
      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'isLanguageServerAvailable',
        lineHint: 2,
        orderHint: 2,
      });

      expect(result.foundAtLine).toBe(3);
    });

    it('should fail with orderHint > 0 when symbol appears once on exact line', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = "export { Foo } from './foo';";

      // orderHint: 1 on a line where Foo appears once → should throw
      // Note: "Foo" (capital) != "foo" (lower) in path, so only 1 code occurrence
      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'Foo',
          lineHint: 1,
          orderHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });
  });

  describe('string and comment context skipping', () => {
    it('should skip symbol occurrence inside single-quoted string', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = "const msg = 'ToolError is bad';";

      // "ToolError" only appears inside a string — should not be found
      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'ToolError',
          lineHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should skip symbol occurrence inside double-quoted string', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = 'const msg = "ToolError is bad";';

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'ToolError',
          lineHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should skip symbol occurrence inside template literal', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = 'const msg = `ToolError is bad`;';

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'ToolError',
          lineHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should skip symbol occurrence inside line comment', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = 'const x = 1; // ToolError here';

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'ToolError',
          lineHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should find code symbol and skip string occurrence on re-export line', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content =
        "export { ToolError, isToolError, toToolError } from './ToolError.js';";

      // orderHint: 0 should find the export specifier (code), NOT the path string
      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'ToolError',
        lineHint: 1,
        orderHint: 0,
      });

      // "export { ToolError," — ToolError starts at char 9
      expect(result.position.character).toBe(9);
    });

    it('should NOT find second code occurrence when only string occurrence remains', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content =
        "export { ToolError, isToolError, toToolError } from './ToolError.js';";

      // orderHint: 1 — only 1 code occurrence of "ToolError" exists (char 9).
      // The second word-boundary match (char 55 in path string) should be skipped.
      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'ToolError',
          lineHint: 1,
          orderHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should handle escaped quotes correctly', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      // The escaped quote does NOT close the string, so ToolError is still inside it
      const content = "const msg = 'it\\'s a ToolError';";

      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'ToolError',
          lineHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should find symbol after a closed string on same line', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = "const msg = 'hello'; const err = new ToolError();";

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'ToolError',
        lineHint: 1,
      });

      // ToolError is in code after the string closes — should be found
      expect(result.position.character).toBe(37);
    });

    it('should prefer code occurrence over string occurrence with orderHint 0', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      // Code symbol first, then same name in string
      const content = "const ToolError = 'ToolError';";

      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'ToolError',
        lineHint: 1,
        orderHint: 0,
      });

      // Should find the code identifier at char 6, not the string at char 19
      expect(result.position.character).toBe(6);
    });

    it('should count only code occurrences for orderHint', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      // Two code occurrences with a string occurrence in between
      const content = "const x = x + 'x is nice' + x;";

      // orderHint: 0 → first code x (char 6)
      const r0 = resolver.resolvePositionFromContent(content, {
        symbolName: 'x',
        lineHint: 1,
        orderHint: 0,
      });
      expect(r0.position.character).toBe(6);

      // orderHint: 1 → second code x (char 10), skipping string occurrence
      const r1 = resolver.resolvePositionFromContent(content, {
        symbolName: 'x',
        lineHint: 1,
        orderHint: 1,
      });
      expect(r1.position.character).toBe(10);

      // orderHint: 2 → third code x (char 28)
      const r2 = resolver.resolvePositionFromContent(content, {
        symbolName: 'x',
        lineHint: 1,
        orderHint: 2,
      });
      expect(r2.position.character).toBe(28);
    });

    it('should find symbol inside template expression ${...} (code context)', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = 'const msg = `Error: ${ToolError.message}`;';

      // ToolError is inside ${...} which is code, not string
      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'ToolError',
        lineHint: 1,
      });

      expect(result.position.character).toBe(22);
    });

    it('should skip symbol in template text but find it in template expression', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = 'const msg = `ToolError: ${ToolError.message}`;';

      // First "ToolError" at char 14 is in template text → skip
      // Second "ToolError" at char 26 is inside ${...} → code → find it
      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'ToolError',
        lineHint: 1,
        orderHint: 0,
      });

      expect(result.position.character).toBe(26);
    });

    it('should skip symbol in template text entirely when no expression match', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = 'const msg = `ToolError happened`;';

      // ToolError is in template text (no ${...}) → skip
      expect(() =>
        resolver.resolvePositionFromContent(content, {
          symbolName: 'ToolError',
          lineHint: 1,
        })
      ).toThrow(SymbolResolutionError);
    });

    it('should handle nested braces inside template expression', () => {
      const resolver = new SymbolResolver({ lineSearchRadius: 0 });
      const content = 'const msg = `${obj.fn({ key: ToolError })}`;';

      // ToolError is inside ${...{ ... }} — nested braces, still code
      const result = resolver.resolvePositionFromContent(content, {
        symbolName: 'ToolError',
        lineHint: 1,
      });

      expect(result.position.character).toBe(29);
    });
  });
});

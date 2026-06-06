import { describe, it, expect } from 'vitest';
import { URI } from 'vscode-uri';

describe('LSP URI Handling', () => {
  describe('URI.file() - toUri equivalent', () => {
    it('should handle Unix paths', () => {
      const uri = URI.file('/users/me/file.ts').toString();
      expect(uri).toBe('file:///users/me/file.ts');
    });

    it('should handle Windows paths', () => {
      const uri = URI.file('C:\\Users\\me\\file.ts').toString();
      expect(uri).toMatch(/^file:\/\/\/[cC]/);
      expect(uri).toContain('Users');
      expect(uri).toContain('file.ts');
    });

    it('should handle paths with spaces', () => {
      const uri = URI.file('/path/with spaces/file.ts').toString();
      expect(uri).toBe('file:///path/with%20spaces/file.ts');
    });

    it('should handle paths with hash character', () => {
      const uri = URI.file('/path/file#1.ts').toString();
      expect(uri).toBe('file:///path/file%231.ts');
    });

    it('should handle paths with percent character', () => {
      const uri = URI.file('/path/file%test.ts').toString();
      expect(uri).toBe('file:///path/file%25test.ts');
    });

    it('should handle Unicode characters', () => {
      const uri = URI.file('/path/файл.ts').toString();
      expect(uri).toContain('file:///path/');
      expect(uri).not.toContain('файл');
    });
  });

  describe('URI.parse().fsPath - fromUri equivalent', () => {
    it('should decode Unix paths', () => {
      const fsPath = URI.parse('file:///users/me/file.ts').fsPath;
      expect(fsPath).toBe('/users/me/file.ts');
    });

    it('should decode paths with spaces', () => {
      const fsPath = URI.parse('file:///path/with%20spaces/file.ts').fsPath;
      expect(fsPath).toBe('/path/with spaces/file.ts');
    });

    it('should decode paths with hash character', () => {
      const fsPath = URI.parse('file:///path/file%231.ts').fsPath;
      expect(fsPath).toBe('/path/file#1.ts');
    });

    it('should decode paths with percent character', () => {
      const fsPath = URI.parse('file:///path/file%25test.ts').fsPath;
      expect(fsPath).toBe('/path/file%test.ts');
    });
  });

  describe('Round-trip conversion', () => {
    const testPaths = [
      '/simple/path/file.ts',
      '/path/with spaces/file.ts',
      '/path/file#1.ts',
      '/path/file%test.ts',
      '/path/with-dash/file.ts',
      '/path/with_underscore/file.ts',
      '/path/深度/file.ts',
    ];

    for (const originalPath of testPaths) {
      it(`should round-trip: ${originalPath}`, () => {
        const uri = URI.file(originalPath).toString();
        const restored = URI.parse(uri).fsPath;
        expect(restored).toBe(originalPath);
      });
    }
  });

  describe('Edge cases', () => {
    it('should handle empty filename', () => {
      const uri = URI.file('/path/to/').toString();
      expect(uri).toBe('file:///path/to/');
    });

    it('should handle root path', () => {
      const uri = URI.file('/').toString();
      expect(uri).toBe('file:///');
    });

    it('should handle deeply nested paths', () => {
      const deepPath =
        '/a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s/t/u/v/w/x/y/z.ts';
      const uri = URI.file(deepPath).toString();
      const restored = URI.parse(uri).fsPath;
      expect(restored).toBe(deepPath);
    });
  });
});

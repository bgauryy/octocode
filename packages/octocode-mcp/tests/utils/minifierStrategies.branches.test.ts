import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  removeComments,
  minifyAggressiveCore,
  minifyWithTerser,
  minifyCSSAsync,
  minifyHTMLAsync,
  minifyCSSCore,
  minifyHTMLCore,
} from '../../src/utils/minifier/minifierStrategies.js';
import type {
  CommentPatternGroup,
  FileTypeMinifyConfig,
} from '../../src/utils/minifier/minifierTypes.js';

const mockMinify = vi.hoisted(() => vi.fn());
vi.mock('terser', () => ({
  minify: mockMinify,
}));

const mockCleanCSSMinify = vi.hoisted(() => vi.fn());
const mockCleanCSSConstructor = vi.hoisted(() => vi.fn());
vi.mock('clean-css', () => {
  return {
    default: class MockCleanCSS {
      constructor(options: any) {
        mockCleanCSSConstructor(options);
      }
      minify(content: string) {
        return mockCleanCSSMinify(content);
      }
    },
  };
});

const mockHtmlMinify = vi.hoisted(() => vi.fn());
vi.mock('html-minifier-terser', () => ({
  minify: mockHtmlMinify,
}));

describe('minifierStrategies - Branch Coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('removeComments - Line 27: undefined comment pattern type', () => {
    it('should handle undefined comment pattern type gracefully', () => {
      const content = 'some code /* comment */';

      const invalidType = 'nonexistent-type' as any as CommentPatternGroup;

      const result = removeComments(content, invalidType);

      expect(result).toBe(content);
    });

    it('should handle array with undefined comment pattern types', () => {
      const content = 'some code /* comment */';
      const invalidTypes = [
        'c-style',
        'nonexistent-type' as any as CommentPatternGroup,
      ];

      const result = removeComments(content, invalidTypes as any);

      expect(result).not.toContain('/* comment */');
    });
  });

  describe('minifyAggressiveCore - Line 79: config.comments is undefined', () => {
    it('should handle config without comments property', () => {
      const content = '  function test() { return true; }  ';
      const config: FileTypeMinifyConfig = {
        strategy: 'aggressive',
      };

      const result = minifyAggressiveCore(content, config);

      expect(result).toBe('function test(){return true;}');
      expect(result).not.toContain('  ');
    });

    it('should handle config with comments set to undefined explicitly', () => {
      const content = '  function test() { return true; }  ';
      const config: FileTypeMinifyConfig = {
        strategy: 'aggressive',
        comments: undefined,
      };

      const result = minifyAggressiveCore(content, config);

      expect(result).toBe('function test(){return true;}');
    });
  });

  describe('minifyWithTerser - Line 238: result.code is falsy', () => {
    it('should fallback to original content when result.code is null', async () => {
      const content = 'function test() { return true; }';
      mockMinify.mockResolvedValue({
        code: null,
      });

      const result = await minifyWithTerser(content);

      expect(result.failed).toBe(false);
      expect(result.content).toBe(content);
    });

    it('should fallback to original content when result.code is undefined', async () => {
      const content = 'function test() { return true; }';
      mockMinify.mockResolvedValue({});

      const result = await minifyWithTerser(content);

      expect(result.failed).toBe(false);
      expect(result.content).toBe(content);
    });

    it('should fallback to original content when result.code is empty string', async () => {
      const content = 'function test() { return true; }';
      mockMinify.mockResolvedValue({
        code: '',
      });

      const result = await minifyWithTerser(content);

      expect(result.failed).toBe(false);
      expect(result.content).toBe(content);
    });

    it('should use result.code when it is a non-empty string', async () => {
      const content = 'function test() { return true; }';
      const minified = 'function test(){return true;}';
      mockMinify.mockResolvedValue({
        code: minified,
      });

      const result = await minifyWithTerser(content);

      expect(result.failed).toBe(false);
      expect(result.content).toBe(minified);
    });
  });

  describe('minifyCSSAsync - Lines 274-303: error fallback path', () => {
    beforeEach(() => {
      const mockObj = { minifyCSSCore };
      vi.spyOn(mockObj, 'minifyCSSCore' as any).mockImplementation(((
        content: string
      ) => {
        return minifyCSSCore(content);
      }) as any);
    });

    it('should fallback to regex minification when CleanCSS throws error', async () => {
      const content = 'body { color: red; /* comment */ }';
      const error = new Error('CleanCSS parse error');

      mockCleanCSSMinify.mockImplementation(() => {
        throw error;
      });

      const result = await minifyCSSAsync(content);

      expect(result.failed).toBe(false);
      expect(result.reason).toContain('CleanCSS fallback');
      expect(result.reason).toContain('CleanCSS parse error');
      expect(result.content).toBe(minifyCSSCore(content));
    });

    it('should fallback when CleanCSS returns errors array', async () => {
      const content = 'body { invalid: syntax; }';

      mockCleanCSSMinify.mockReturnValue({
        styles: '',
        errors: ['Parse error: unexpected token'],
        warnings: [],
      });

      const result = await minifyCSSAsync(content);

      expect(result.failed).toBe(false);
      expect(result.reason).toContain('CleanCSS fallback');
      expect(result.reason).toContain('unexpected token');
      expect(result.content).toBe(minifyCSSCore(content));
    });

    it('should fallback when error is not an Error instance', async () => {
      const content = 'body { color: red; }';

      mockCleanCSSMinify.mockImplementation(() => {
        throw 'String error';
      });

      const result = await minifyCSSAsync(content);

      expect(result.failed).toBe(false);
      expect(result.reason).toContain('CleanCSS fallback');
      expect(result.reason).toContain('unknown');
      expect(result.content).toBe(minifyCSSCore(content));
    });

    it('should successfully minify when CleanCSS works', async () => {
      const content = 'body { color: red; }';
      const minified = 'body{color:red}';

      mockCleanCSSMinify.mockReturnValue({
        styles: minified,
        errors: [],
        warnings: [],
      });

      const result = await minifyCSSAsync(content);

      expect(result.failed).toBe(false);
      expect(result.reason).toBeUndefined();
      expect(result.content).toBe(minified);
    });
  });

  describe('minifyHTMLAsync - Lines 274-303: error fallback path', () => {
    it('should fallback to regex minification when html-minifier-terser throws error', async () => {
      const content = '<div>  Test  </div><!-- comment -->';
      const error = new Error('HTML parse error');

      mockHtmlMinify.mockRejectedValue(error);

      const result = await minifyHTMLAsync(content);

      expect(result.failed).toBe(false);
      expect(result.reason).toContain('html-minifier fallback');
      expect(result.reason).toContain('HTML parse error');
      expect(result.content).toBe(minifyHTMLCore(content));
    });

    it('should fallback when error is not an Error instance', async () => {
      const content = '<div>Test</div>';

      mockHtmlMinify.mockRejectedValue('String error');

      const result = await minifyHTMLAsync(content);

      expect(result.failed).toBe(false);
      expect(result.reason).toContain('html-minifier fallback');
      expect(result.reason).toContain('unknown');
      expect(result.content).toBe(minifyHTMLCore(content));
    });

    it('should successfully minify when html-minifier-terser works', async () => {
      const content = '<div>  Test  </div><!-- comment -->';
      const minified = '<div>Test</div>';

      mockHtmlMinify.mockResolvedValue(minified);

      const result = await minifyHTMLAsync(content);

      expect(result.failed).toBe(false);
      expect(result.reason).toBeUndefined();
      expect(result.content).toBe(minified);
    });

    it('should fallback when html-minifier-terser rejects with null', async () => {
      const content = '<div>Test</div>';

      mockHtmlMinify.mockRejectedValue(null);

      const result = await minifyHTMLAsync(content);

      expect(result.failed).toBe(false);
      expect(result.reason).toContain('html-minifier fallback');
      expect(result.reason).toContain('unknown');
      expect(result.content).toBe(minifyHTMLCore(content));
    });
  });
});

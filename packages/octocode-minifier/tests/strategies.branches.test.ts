import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  removeComments,
  minifyAggressiveCore,
  minifyWithTerser,
  minifyCSSAsync,
  minifyHTMLAsync,
  minifyCSSCore,
  minifyHTMLCore,
} from '@octocodeai/octocode-minifier';
import type {
  CommentPatternGroup,
  FileTypeMinifyConfig,
} from '@octocodeai/octocode-minifier';

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

  describe('removeComments - undefined comment pattern type', () => {
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

  describe('removeComments - string-aware code families', () => {
    it.each([
      {
        label: 'c-style',
        type: 'c-style' as const,
        content:
          'const url = "https://example.com"; // remove\nconst marker = "/* keep */";\nconst regex = /[/*]{2}/g;\nconst escapedRegex = /https?:\\/\\/example\\.com/;\nconst rust = r#"quote " /* keep raw */ // keep raw"#;\nconst verbatim = @"quote "" // keep verbatim";\nconst interpolated = $@"quote "" // keep interpolated";\nconst escaped = "quote \\" // keep";\n/* remove block */\nconst done = true;',
        kept: [
          '"https://example.com"',
          '"/* keep */"',
          '/[/*]{2}/g',
          '/https?:\\/\\/example\\.com/',
          'r#"quote " /* keep raw */ // keep raw"#',
          '@"quote "" // keep verbatim"',
          '$@"quote "" // keep interpolated"',
          '"quote \\" // keep"',
          'const done = true',
        ],
        removed: ['// remove', 'remove block */'],
      },
      {
        label: 'hash',
        type: 'hash' as const,
        content:
          '#!/bin/bash\necho "# keep"\ntext = """# keep triple"""\nvalue = 1 # remove',
        kept: ['#!/bin/bash', '"# keep"', '"""# keep triple"""'],
        removed: ['# remove'],
      },
      {
        label: 'sql',
        type: 'sql' as const,
        content:
          "SELECT '-- keep'; -- remove\n/* remove block */\nSELECT '/* keep */';",
        kept: ["'-- keep'", "'/* keep */'"],
        removed: ['-- remove', 'remove block'],
      },
      {
        label: 'html',
        type: 'html' as const,
        content:
          '<div data-note="<!-- keep -->">keep</div>\n<!-- remove block -->',
        kept: ['"<!-- keep -->"', '>keep</div>'],
        removed: ['remove block'],
      },
      {
        label: 'lua',
        type: 'lua' as const,
        content:
          'local s = "-- keep"\nlocal b = "--[[ keep ]]"\nlocal x = 1 -- remove\n--[[ remove block ]]',
        kept: ['"-- keep"', '"--[[ keep ]]"', 'local x = 1'],
        removed: ['-- remove', 'remove block'],
      },
      {
        label: 'haskell',
        type: 'haskell' as const,
        content:
          'main = putStrLn "-- keep" -- remove\nname = "{- keep -}"\n{- remove block -}',
        kept: ['"-- keep"', '"{- keep -}"'],
        removed: ['-- remove', 'remove block'],
      },
      {
        label: 'semicolon',
        type: 'semicolon' as const,
        content: 'value = "; keep" ; remove\nnext = 1',
        kept: ['"; keep"', 'next = 1'],
        removed: ['; remove'],
      },
      {
        label: 'wasm-text',
        type: 'wasm-text' as const,
        content:
          '(module\n  (data ";; keep") ;; remove\n  (; remove block ;)\n  (func (export "run"))\n)',
        kept: ['";; keep"', '(func (export "run"))'],
        removed: [';; remove', 'remove block'],
      },
      {
        label: 'percent',
        type: 'percent' as const,
        content: 'Value = "% keep" % remove\nnext().',
        kept: ['"% keep"', 'next().'],
        removed: ['% remove'],
      },
      {
        label: 'template',
        type: 'template' as const,
        content:
          '<div data-note="{{! keep }}">{{ name }}</div>\n{{!-- remove hbs --}}\n<%# remove ejs %>\n{# remove twig #}',
        kept: ['"{{! keep }}"', '{{ name }}'],
        removed: ['remove hbs', 'remove ejs', 'remove twig'],
      },
      {
        label: 'haml',
        type: 'haml' as const,
        content: '%div{title: "-# keep"}\n-# remove\n%span keep',
        kept: ['"-# keep"', '%span keep'],
        removed: ['-# remove'],
      },
      {
        label: 'slim',
        type: 'slim' as const,
        content: 'a href="/keep" keep\n/ remove\nspan keep',
        kept: ['"/keep"', 'span keep'],
        removed: ['/ remove'],
      },
    ])('$label comments outside strings only', testCase => {
      const result = removeComments(testCase.content, testCase.type);

      for (const expected of testCase.kept) {
        expect(result).toContain(expected);
      }
      for (const removed of testCase.removed) {
        expect(result).not.toContain(removed);
      }
    });

    it('leaves unterminated regex-like text intact while stripping later comments', () => {
      const content = 'const re = /unterminated\nconst x = 1; // remove';
      const result = removeComments(content, 'c-style');

      expect(result).toContain('/unterminated');
      expect(result).toContain('const x = 1;');
      expect(result).not.toContain('// remove');
    });

    it('leaves end-of-file regex-like text intact when no closing slash exists', () => {
      const content = 'const re = /unterminated';
      const result = removeComments(content, 'c-style');

      expect(result).toBe(content);
    });

    it('leaves unterminated verbatim strings intact', () => {
      const content = 'const value = @"unterminated // keep';
      const result = removeComments(content, 'c-style');

      expect(result).toBe(content);
    });
  });

  describe('minifyAggressiveCore - config.comments is undefined', () => {
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

  describe('minifyWithTerser - result.code is falsy', () => {
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

  describe('minifyCSSAsync - error fallback path', () => {
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

  describe('minifyHTMLAsync - error fallback path', () => {
    it('returns empty or whitespace-only content without calling html-minifier-terser', async () => {
      const empty = await minifyHTMLAsync('');
      const whitespace = await minifyHTMLAsync(' \n\t ');

      expect(empty).toEqual({ content: '', failed: false });
      expect(whitespace).toEqual({ content: ' \n\t ', failed: false });
      expect(mockHtmlMinify).not.toHaveBeenCalled();
    });

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

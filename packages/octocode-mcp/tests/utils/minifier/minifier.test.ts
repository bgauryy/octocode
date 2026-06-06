import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  minifyContent,
  MINIFY_CONFIG,
} from '../../../src/utils/minifier/minifier.js';

const mockMinify = vi.hoisted(() => vi.fn());
vi.mock('terser', () => ({
  minify: mockMinify,
}));

const mockCleanCSSMinify = vi.hoisted(() => vi.fn());
vi.mock('clean-css', () => {
  return {
    default: class MockCleanCSS {
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

describe('MinifierV2', () => {
  beforeEach(() => {
    mockMinify.mockReset();
    mockCleanCSSMinify.mockReset();
    mockHtmlMinify.mockReset();

    mockCleanCSSMinify.mockReturnValue({ styles: '', errors: [] });
    mockHtmlMinify.mockResolvedValue('');
  });

  describe('Configuration', () => {
    it('should have proper comment patterns defined', () => {
      expect(MINIFY_CONFIG.commentPatterns['c-style']).toHaveLength(3);
      expect(MINIFY_CONFIG.commentPatterns.hash).toHaveLength(2);
      expect(MINIFY_CONFIG.commentPatterns.html).toHaveLength(1);
    });

    it('should map file extensions to correct strategies', () => {
      expect(MINIFY_CONFIG.fileTypes.js!.strategy).toBe('terser');
      expect(MINIFY_CONFIG.fileTypes.py!.strategy).toBe('conservative');
      expect(MINIFY_CONFIG.fileTypes.html!.strategy).toBe('aggressive');
      expect(MINIFY_CONFIG.fileTypes.json!.strategy).toBe('json');
    });
  });

  describe('Terser Strategy', () => {
    it('should use terser for JavaScript files', async () => {
      const jsCode = 'function test() { return true; }';
      mockMinify.mockResolvedValue({
        code: 'function test(){return true;}',
      });

      const result = await minifyContent(jsCode, 'test.js');

      expect(result.type).toBe('terser');
      expect(result.failed).toBe(false);
      expect(result.content).toBe('function test(){return true;}');
      expect(mockMinify).toHaveBeenCalledWith(jsCode, expect.any(Object));
    });

    it('should handle terser failures gracefully', async () => {
      const jsCode = 'invalid js {{{';
      mockMinify.mockRejectedValue(new Error('Parse error'));

      const result = await minifyContent(jsCode, 'test.js');

      expect(result.type).toBe('failed');
      expect(result.failed).toBe(true);
      expect(result.content).toBe(jsCode);
    });
  });

  describe('TypeScript Conservative Strategy', () => {
    it('should use conservative strategy for TypeScript files (preserves readability)', async () => {
      const tsCode = `// Single line comment
interface User {
  name: string;
  age: number;
}

/* Multi-line comment
   describing the function */
function greet(user: User): string {
  return \`Hello, \${user.name}!\`;
}`;

      const result = await minifyContent(tsCode, 'test.ts');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('// Single line comment');
      expect(result.content).not.toContain('/* Multi-line comment');
      expect(result.content).not.toContain('describing the function */');

      expect(result.content).toContain('interface User');
      expect(result.content).toContain('name: string');
      expect(result.content).toContain('function greet(user: User): string');

      expect(mockMinify).not.toHaveBeenCalled();
    });

    it('should handle TSX files with conservative strategy', async () => {
      const tsxCode = `// Component comment
import React from 'react';

interface Props {
  title: string;
}

/* Main component */
export const Header: React.FC<Props> = ({ title }) => {
  return <h1>{title}</h1>;
};`;

      const result = await minifyContent(tsxCode, 'Header.tsx');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('// Component comment');
      expect(result.content).not.toContain('/* Main component */');

      expect(result.content).toContain('interface Props');
      expect(result.content).toContain('title: string');
      expect(result.content).toContain('React.FC<Props>');

      expect(mockMinify).not.toHaveBeenCalled();
    });

    it('should handle complex TypeScript with generics preserving structure', async () => {
      const tsCode = `// Utility types
type Partial<T> = {
  [P in keyof T]?: T[P];
};

/* Generic function */
async function fetchData<T extends object>(
  url: string,
  options?: RequestInit
): Promise<T> {
  const response = await fetch(url, options);
  return response.json() as T;
}

enum Status {
  Pending = 'pending',
  Active = 'active',
  Done = 'done'
}`;

      const result = await minifyContent(tsCode, 'utils.ts');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('// Utility types');
      expect(result.content).not.toContain('/* Generic function */');
      expect(result.content).not.toContain('// Enum definition');

      expect(result.content).toContain('type Partial<T>');
      expect(result.content).toContain('keyof T');
      expect(result.content).toContain(
        'async function fetchData<T extends object>'
      );
      expect(result.content).toContain('Promise<T>');
      expect(result.content).toContain('enum Status');
    });

    it('should never fail on TypeScript files (conservative is safe)', async () => {
      const tsCode = `
interface ComplexType<T extends Record<string, unknown>> {
  data: T;
  metadata: {
    createdAt: Date;
    updatedAt?: Date;
  };
}

type ExtractKeys<T> = T extends Record<infer K, unknown> ? K : never;

const handler: <T>(input: T) => T = (input) => input;
`;

      const result = await minifyContent(tsCode, 'complex.ts');

      expect(result.failed).toBe(false);
      expect(result.type).toBe('conservative');

      expect(result.content.length).toBeLessThan(tsCode.length);
    });
  });

  describe('Conservative Strategy (Indentation-Sensitive)', () => {
    it('should preserve Python indentation structure', async () => {
      const pythonCode = `def hello():
    # This is a comment
    if True:
        print("Hello")

        # Another comment
        return True

# Top level comment
class MyClass:
    pass`;

      const result = await minifyContent(pythonCode, 'test.py');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('# This is a comment');
      expect(result.content).not.toContain('# Another comment');
      expect(result.content).not.toContain('# Top level comment');

      expect(result.content).toContain('def hello():');
      expect(result.content).toContain('    if True:');
      expect(result.content).toContain('        print("Hello")');
      expect(result.content).toContain('        return True');
    });

    it('should handle YAML conservatively', async () => {
      const yamlCode = `# YAML configuration
version: '3.8'
services:
  web:
    # Web service config
    image: nginx:latest
    ports:
      - "80:80"

  # Database service
  db:
    image: postgres:13`;

      const result = await minifyContent(yamlCode, 'docker-compose.yml');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('# YAML configuration');
      expect(result.content).not.toContain('# Web service config');
      expect(result.content).not.toContain('# Database service');

      expect(result.content).toContain("version: '3.8'");
      expect(result.content).toContain('services:');
      expect(result.content).toContain('  web:');
      expect(result.content).toContain('    image: nginx:latest');
    });
  });

  describe('Aggressive Strategy', () => {
    it('should aggressively minify HTML', async () => {
      const htmlCode = `<!DOCTYPE html>
<!-- This is a comment -->
<html>
  <head>
    <title>Test</title>
  </head>
  <body>
    <h1>Hello World</h1>
    <p>This is a paragraph</p>
  </body>
</html>`;

      const expectedMinified =
        '<!DOCTYPE html><html><head><title>Test</title></head>' +
        '<body><h1>Hello World</h1><p>This is a paragraph</p></body></html>';
      mockHtmlMinify.mockResolvedValue(expectedMinified);

      const result = await minifyContent(htmlCode, 'test.html');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('<!-- This is a comment -->');
      expect(result.content).toBe(expectedMinified);
    });

    it('should aggressively minify CSS', async () => {
      const cssCode = `/* Main styles */
.container {
  /* Container styles */
  padding: 20px;
  margin: 0 auto;
  max-width: 1200px;
}

.button {
  background-color: blue;
  color: white;
  /* Button styles */
  border: none;
}`;

      mockCleanCSSMinify.mockReturnValue({
        styles:
          '.container{padding:20px;margin:0 auto;max-width:1200px}' +
          '.button{background-color:#00f;color:#fff;border:none}',
        errors: [],
      });

      const result = await minifyContent(cssCode, 'styles.css');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('/* Main styles */');
      expect(result.content).not.toContain('/* Container styles */');
      expect(result.content).not.toContain('/* Button styles */');

      expect(result.content).toContain(
        '.container{padding:20px;margin:0 auto;max-width:1200px}'
      );
      expect(result.content).toContain(
        '.button{background-color:#00f;color:#fff;border:none}'
      );
    });

    it('should handle Go code conservatively (readable, newline-preserving)', async () => {
      const goCode = `package main

import "fmt"

func main() {
    /*
     * Print hello world
     */
    fmt.Println("Hello, World!")

    var x = 42
}`;

      const result = await minifyContent(goCode, 'main.go');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('// Main function');
      expect(result.content).not.toContain('Print hello world');
      expect(result.content).not.toContain('// Another comment');

      expect(result.content).toContain('\n');
      expect(result.content).toContain('func main() {');
      expect(result.content).not.toMatch(/import "fmt" func main/);
    });
  });

  describe('JSON Strategy', () => {
    it('should minify valid JSON', async () => {
      const jsonCode = `{
  "name": "test-package",
  "version": "1.0.0",
  "dependencies": {
    "lodash": "^4.17.21"
  }
}`;

      const result = await minifyContent(jsonCode, 'package.json');

      expect(result.type).toBe('json');
      expect(result.failed).toBe(false);
      expect(result.content).toBe(
        '{"name":"test-package","version":"1.0.0","dependencies":{"lodash":"^4.17.21"}}'
      );
    });

    it('should handle JSON with comments (JSONC)', async () => {
      const jsonWithComments = `{
  "name": "test",
  "version": "1.0.0"
}`;

      const result = await minifyContent(jsonWithComments, 'config.json');

      expect(result.type).toBe('json');
      expect(result.failed).toBe(false);
      expect(result.content).toBe('{"name":"test","version":"1.0.0"}');
    });

    it('should return trimmed content for unparseable JSON', async () => {
      const invalidJson = `{
  "name": "test",
  "missing_comma": true
  "version": "1.0.0"
}`;

      const result = await minifyContent(invalidJson, 'invalid.json');

      expect(result.type).toBe('json');
      expect(result.failed).toBe(false);
      expect(result.content).toContain('"name": "test",');
      expect(result.content).toContain('"missing_comma": true');
    });

    it('should preserve spaces within strings when parsing JSONC', async () => {
      const jsonWithSpaces = `{
  "key": "value   with   multiple   spaces",
  "other": "data"
}`;
      const result = await minifyContent(jsonWithSpaces, 'config.json');

      expect(result.type).toBe('json');
      expect(result.failed).toBe(false);
      expect(result.content).toContain('"value   with   multiple   spaces"');
    });
  });

  describe('Multi-language Comment Support', () => {
    it('should handle PHP with multiple comment types', async () => {
      const phpCode = `<?php
/* Multi-line comment */
# Hash comment
function test() {
    return true;
}
?>`;

      const result = await minifyContent(phpCode, 'test.php');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('// Single line comment');
      expect(result.content).not.toContain('/* Multi-line comment */');
      expect(result.content).not.toContain('# Hash comment');
      expect(result.content).not.toContain('// Inline comment');
    });

    it('should handle SQL comments', async () => {
      const sqlCode = `-- This is a SQL comment
SELECT * FROM users
/* Multi-line SQL comment
   spanning multiple lines */
WHERE active = 1;
-- Another comment`;

      const result = await minifyContent(sqlCode, 'query.sql');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);

      expect(result.content).not.toContain('-- This is a SQL comment');
      expect(result.content).not.toContain('/* Multi-line SQL comment');
      expect(result.content).not.toContain('-- Another comment');

      expect(result.content).toContain('SELECT * FROM users WHERE active = 1;');
    });
  });

  describe('Markdown Strategy', () => {
    it('should use markdown minification for .md files', async () => {
      const markdown = `# Title

Paragraph with **bold** text.

## Section 2

Another paragraph with *italic* text.

- List item 1
- List item 2

`;

      const result = await minifyContent(markdown, 'readme.md');

      expect(result.type).toBe('markdown');
      expect(result.failed).toBe(false);
      expect(result.content).toContain('# Title');
      expect(result.content).toContain('**bold**');
    });

    it('should handle .markdown extension', async () => {
      const markdown = '# Header\n\nContent here.';
      const result = await minifyContent(markdown, 'docs.markdown');

      expect(result.type).toBe('markdown');
      expect(result.failed).toBe(false);
    });
  });

  describe('Unknown File Types', () => {
    it('should fallback to general strategy for unknown extensions', async () => {
      const unknownContent = `# Some config file
setting1=value1
setting2=value2

# Another section


setting3=value3    `;

      const result = await minifyContent(unknownContent, 'unknown.xyz');

      expect(result.type).toBe('general');
      expect(result.failed).toBe(false);

      expect(result.content).toContain('setting1=value1');
      expect(result.content).toContain('setting2=value2');
      expect(result.content).toContain('setting3=value3');

      expect(result.content).not.toMatch(/[ \t]+$/m);
      expect(result.content).not.toMatch(/\n\n\n+/);
    });

    it('should treat indentation-sensitive filenames conservatively (no extension)', async () => {
      const makefile = `# Simple Makefile
build:
	@echo "Building"

test:
	@echo "Testing"`;

      const result = await minifyContent(makefile, 'Makefile');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);
      expect(result.content).toContain('\t@echo "Building"');
      expect(result.content).toContain('\t@echo "Testing"');
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty content', async () => {
      const result = await minifyContent('', 'empty.js');
      expect(result.content).toBe('');
      expect(result.failed).toBe(false);
    });

    it('should handle content with only whitespace', async () => {
      const result = await minifyContent('   \n\n\t  \n  ', 'whitespace.txt');
      expect(result.content).toBe('');
      expect(result.failed).toBe(false);
    });

    it('should handle content with only comments', async () => {
      const result = await minifyContent(
        '# Comment 1\n# Comment 2\n# Comment 3',
        'comments.sh'
      );
      expect(result.content).toBe('');
      expect(result.failed).toBe(false);
    });
  });

  describe('Size Limit Tests', () => {
    it('should reject content larger than 1MB', async () => {
      const oneMB = 1024 * 1024;
      const largeContent = 'x'.repeat(oneMB + 100);

      const result = await minifyContent(largeContent, 'large.js');

      expect(result.failed).toBe(true);
      expect(result.type).toBe('failed');
      expect(result.content).toBe(largeContent);
    });

    it('should accept content exactly at 1MB limit', async () => {
      const oneMB = 1024 * 1024;
      const limitContent = 'a'.repeat(oneMB);

      const result = await minifyContent(limitContent, 'limit.txt');

      expect(result.failed).toBe(false);
      expect(result.type).toBe('general');
      expect(result.content.length).toBeLessThanOrEqual(limitContent.length);
    });
  });

  describe('Error Handling', () => {
    it('should handle non-string content gracefully', async () => {
      const result = await minifyContent(null as unknown as string, 'test.txt');

      expect(result.failed).toBe(true);
      expect(result.type).toBe('failed');
      expect(result.reason).toContain('Unexpected minification error');
    });

    it('should handle undefined content gracefully', async () => {
      const result = await minifyContent(
        undefined as unknown as string,
        'test.txt'
      );

      expect(result.failed).toBe(true);
      expect(result.type).toBe('failed');
      expect(result.reason).toContain('Unexpected minification error');
    });

    it('should return original content when an unexpected error occurs', async () => {
      const problematicContent = {
        toString() {
          throw new Error('Cannot convert to string');
        },
      };

      const result = await minifyContent(
        problematicContent as unknown as string,
        'test.txt'
      );

      expect(result.failed).toBe(true);
      expect(result.type).toBe('failed');
    });

    it('should handle non-Error exceptions in terser failure', async () => {
      mockMinify.mockRejectedValue('String error');

      const result = await minifyContent('const x = 1;', 'test.js');

      expect(result.failed).toBe(true);
      expect(result.type).toBe('failed');
      expect(result.reason).toContain('Unknown error');
    });
  });

  describe('CSS Error Handling', () => {
    it('should use CleanCSS for CSS minification', async () => {
      const cssCode = `.test { color: red; }`;
      mockCleanCSSMinify.mockReturnValue({
        styles: '.test{color:red}',
        errors: [],
      });

      const result = await minifyContent(cssCode, 'test.css');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
      expect(result.content).toBe('.test{color:red}');
      expect(mockCleanCSSMinify).toHaveBeenCalledWith(cssCode);
    });

    it('should fall back to regex minification when CleanCSS throws and include reason', async () => {
      const cssCode = `.test { color: red; /* comment */ }`;
      mockCleanCSSMinify.mockImplementation(() => {
        throw new Error('CleanCSS internal error');
      });

      const result = await minifyContent(cssCode, 'test.css');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
      expect(result.reason).toContain('CleanCSS fallback');
      expect(result.reason).toContain('CleanCSS internal error');
      expect(result.content).not.toContain('/* comment */');
    });

    it('should fall back to regex minification when CleanCSS returns errors', async () => {
      const cssCode = `.test { color: red; }`;
      mockCleanCSSMinify.mockReturnValue({
        styles: '',
        errors: ['Invalid CSS syntax'],
      });

      const result = await minifyContent(cssCode, 'test.css');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
    });

    it('should minify complex CSS with selectors', async () => {
      const cssCode = `
        .selector-one,
        .selector-two {
          color: red;
          background-color: blue;
          /* Multiple properties */
          border: 1px solid black;
        }
      `;
      mockCleanCSSMinify.mockReturnValue({
        styles:
          '.selector-one,.selector-two{color:red;' +
          'background-color:#00f;border:1px solid #000}',
        errors: [],
      });

      const result = await minifyContent(cssCode, 'complex.css');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
    });

    it('should handle LESS files with aggressive strategy', async () => {
      const lessCode = `
        @color: red;
        .test {
          /* comment */
          color: @color;
        }
      `;
      mockCleanCSSMinify.mockReturnValue({
        styles: '@color:red;.test{color:@color}',
        errors: [],
      });

      const result = await minifyContent(lessCode, 'styles.less');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
    });

    it('should handle SCSS files with aggressive strategy', async () => {
      const scssCode = `
        $color: blue;
        .test {
          /* comment */
          color: $color;
        }
      `;
      mockCleanCSSMinify.mockReturnValue({
        styles: '$color:blue;.test{color:$color}',
        errors: [],
      });

      const result = await minifyContent(scssCode, 'styles.scss');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
    });
  });

  describe('HTML Error Handling', () => {
    it('should use html-minifier-terser for HTML minification', async () => {
      const htmlCode = `<html><body>Test</body></html>`;
      mockHtmlMinify.mockResolvedValue('<html><body>Test</body></html>');

      const result = await minifyContent(htmlCode, 'test.html');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
      expect(mockHtmlMinify).toHaveBeenCalledWith(htmlCode, expect.any(Object));
    });

    it('should fall back to regex minification when html-minifier-terser throws and include reason', async () => {
      const htmlCode = `<html><!-- comment --><body>Test</body></html>`;
      mockHtmlMinify.mockRejectedValue(new Error('html-minifier parse error'));

      const result = await minifyContent(htmlCode, 'test.html');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
      expect(result.reason).toContain('html-minifier fallback');
      expect(result.reason).toContain('html-minifier parse error');
      expect(result.content).not.toContain('<!-- comment -->');
    });

    it('should handle HTM extension with HTML strategy', async () => {
      const htmCode = `<html>
        <body>
          <!-- comment -->
          <p>Test</p>
        </body>
      </html>`;
      mockHtmlMinify.mockResolvedValue('<html><body><p>Test</p></body></html>');

      const result = await minifyContent(htmCode, 'test.htm');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
    });
  });

  describe('Comment Pattern Error Recovery', () => {
    it('should continue processing when a pattern fails', async () => {
      const phpCode = `<?php
/* block */
echo "test";
?>`;

      const result = await minifyContent(phpCode, 'test.php');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);
      expect(result.content).not.toContain('// comment');
    });
  });

  describe('Default Fallback Strategy', () => {
    it('should use default fallback for file with no extension', async () => {
      const content = 'content for extensionless file    \n\n';
      const result = await minifyContent(content, 'SOMEFILE');

      expect(result.failed).toBe(false);
    });

    it('should use general minification for completely unknown formats', async () => {
      const content = 'data: value\nmore: stuff\n\n\n';
      const result = await minifyContent(content, 'file.zzzzzzz');

      expect(result.type).toBe('general');
      expect(result.failed).toBe(false);
    });
  });

  describe('Switch Default Branch Coverage', () => {
    it('should handle file types that fall through to default case via getFileConfig returning unknown strategy', async () => {
      const content = 'some test content with    spaces and\n\n\nlines';
      const result = await minifyContent(content, 'test.xyzabc123');

      expect(result.type).toBe('general');
      expect(result.failed).toBe(false);
    });
  });

  describe('Catch Block Coverage', () => {
    it('should catch errors in minifyGeneral and return original content', async () => {
      const complexContent =
        'Valid content \r\n with\t\t  mixed  \n\n\n\n whitespace';
      const result = await minifyContent(complexContent, 'test.unknownext');

      expect(result.type).toBe('general');
      expect(result.failed).toBe(false);
      expect(result.content).not.toContain('\r\n');
    });

    it('should catch errors in minifyMarkdown and return original content', async () => {
      const markdownContent =
        '# Test\n\n```\ncode block\n```\n\n- item\n  - nested';
      const result = await minifyContent(markdownContent, 'test.md');

      expect(result.type).toBe('markdown');
      expect(result.failed).toBe(false);
    });

    it('should handle markdown with HTML comments in edge cases', async () => {
      const markdownWithComments = `# Title
<!-- Hidden comment that spans
multiple lines -->
Content here.

| Col1 | Col2 |
|------|------|
| a    | b    |`;

      const result = await minifyContent(markdownWithComments, 'readme.md');

      expect(result.type).toBe('markdown');
      expect(result.failed).toBe(false);
      expect(result.content).not.toContain('Hidden comment');
    });
  });

  describe('Additional File Type Coverage', () => {
    it('should handle CSV files with conservative strategy (no comments config)', async () => {
      const csvContent = `name,age,city
John,30,NYC
# This is NOT a comment because CSV has no comment config
Jane,25,LA    
Bob,35,Chicago`;

      const result = await minifyContent(csvContent, 'data.csv');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);
      expect(result.content).toContain('# This is NOT a comment');
      expect(result.content).not.toMatch(/ +$/m);
    });

    it('should handle TXT files with general strategy (no comments)', async () => {
      const txtContent = `Some plain text content

With multiple blank lines


And trailing spaces    `;

      const result = await minifyContent(txtContent, 'notes.txt');

      expect(result.type).toBe('general');
      expect(result.failed).toBe(false);
      expect(result.content).not.toMatch(/\n{3,}/);
      expect(result.content).not.toMatch(/ {2,}$/m);
    });

    it('should handle LOG files with general strategy', async () => {
      const logContent = `[INFO] Starting application
[DEBUG] Loading config

[ERROR] Something went wrong   `;

      const result = await minifyContent(logContent, 'app.log');

      expect(result.type).toBe('general');
      expect(result.failed).toBe(false);
    });

    it('should handle Dockerfile conservatively', async () => {
      const dockerfile = `# Dockerfile comment
FROM node:18
RUN npm install

# Another comment
COPY . .`;

      const result = await minifyContent(dockerfile, 'Dockerfile');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);
      expect(result.content).toContain('FROM node:18');
    });

    it('should handle Jenkinsfile conservatively', async () => {
      const jenkinsfile = `# Pipeline definition
pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                echo 'Building...'
            }
        }
    }
}`;

      const result = await minifyContent(jenkinsfile, 'Jenkinsfile');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);
    });

    it('should handle Vagrantfile conservatively', async () => {
      const vagrantfile = `# Vagrant config
Vagrant.configure("2") do |config|
    config.vm.box = "ubuntu/focal64"
end`;

      const result = await minifyContent(vagrantfile, 'Vagrantfile');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);
    });

    it('should handle various indentation-sensitive filenames', async () => {
      const makefileContent = `# Build
build:
	echo "building"`;

      const filenames = ['Rakefile', 'Gemfile', 'Podfile', 'Fastfile'];

      for (const filename of filenames) {
        const result = await minifyContent(makefileContent, filename);
        expect(result.type).toBe('conservative');
        expect(result.failed).toBe(false);
      }
    });

    it('should handle Haskell files with conservative strategy', async () => {
      const haskellCode = `-- Haskell comment
module Main where

{- Block comment
   spanning lines -}
main :: IO ()
main = putStrLn "Hello"`;

      const result = await minifyContent(haskellCode, 'Main.hs');

      expect(result.type).toBe('conservative');
      expect(result.failed).toBe(false);
      expect(result.content).not.toContain('-- Haskell comment');
      expect(result.content).not.toContain('Block comment');
    });

    it('should handle Lua files with aggressive strategy', async () => {
      const luaCode = `-- Lua single line comment
local x = 1
--[[ Block comment
spanning lines ]]
print(x)`;

      const result = await minifyContent(luaCode, 'script.lua');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
    });

    it('should handle template files (handlebars, twig, etc)', async () => {
      const hbsContent = `{{!-- Comment --}}
<div>
  {{! Another comment }}
  <p>{{name}}</p>
</div>`;

      const result = await minifyContent(hbsContent, 'template.hbs');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
      expect(result.content).not.toContain('Comment');
    });

    it('should handle Terraform files with multiple comment types', async () => {
      const tfContent = `# Hash comment
/* Block comment */
resource "aws_instance" "example" {
  ami           = "ami-12345"
  instance_type = "t2.micro"
}`;

      const result = await minifyContent(tfContent, 'main.tf');

      expect(result.type).toBe('aggressive');
      expect(result.failed).toBe(false);
    });
  });
});

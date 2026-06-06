import { describe, it, expect } from 'vitest';
import { minifyContentSync } from '../../../src/utils/minifier/minifier.js';

describe('minifierSync', () => {
  describe('minifyContentSync', () => {
    describe('JavaScript/TypeScript minification', () => {
      it('should minify JavaScript files', () => {
        const content = `
          const foo = 'bar';
          /* Multi-line
             comment */
          function test() {
            return foo;
          }
        `;

        const result = minifyContentSync(content, 'test.js');

        expect(result).not.toContain('// This is a comment');
        expect(result).not.toContain('Multi-line');
        expect(result.length).toBeLessThan(content.length);
      });

      it('should minify TypeScript files', () => {
        const content = `
          interface Foo {
            bar: string;
          }
          const x: Foo = { bar: 'test' };
        `;

        const result = minifyContentSync(content, 'test.ts');

        expect(result).not.toContain('// Comment');
      });

      it('should minify JSX files', () => {
        const content = `
          const App = () => {
            return <div>Hello</div>;
          };
        `;

        const result = minifyContentSync(content, 'App.jsx');
        expect(result.length).toBeLessThan(content.length);
      });

      it('should minify TSX files', () => {
        const content = `
          const App: React.FC = () => {
            return <div>Hello</div>;
          };
        `;

        const result = minifyContentSync(content, 'App.tsx');
        expect(result.length).toBeLessThan(content.length);
      });

      it('should minify MJS files', () => {
        const content = `
          export const foo = 'bar';
        `;

        const result = minifyContentSync(content, 'test.mjs');
        expect(result.length).toBeLessThan(content.length);
      });

      it('should minify CJS files', () => {
        const content = `
          module.exports = { foo: 'bar' };
        `;

        const result = minifyContentSync(content, 'test.cjs');
        expect(result.length).toBeLessThan(content.length);
      });
    });

    describe('JSON minification', () => {
      it('should minify valid JSON', () => {
        const content = `{
          "name": "test",
          "version": "1.0.0"
        }`;

        const result = minifyContentSync(content, 'package.json');

        expect(result).toBe('{"name":"test","version":"1.0.0"}');
      });

      it('should return original content for invalid JSON', () => {
        const content = '{ invalid json }';

        const result = minifyContentSync(content, 'test.json');

        expect(result).toBe(content);
      });
    });

    describe('CSS minification', () => {
      it('should minify CSS files', () => {
        const content = `
          /* Main styles */
          .container {
            padding: 10px;
            margin: 0;
          }
        `;

        const result = minifyContentSync(content, 'styles.css');

        expect(result).not.toContain('/* Main styles */');
        expect(result).toContain('.container');
      });

      it('should minify SCSS files', () => {
        const content = `
          /* Variables */
          $primary: blue;
          .btn { color: $primary; }
        `;

        const result = minifyContentSync(content, 'styles.scss');
        expect(result.length).toBeLessThan(content.length);
      });

      it('should minify SASS files', () => {
        const content = `
          /* Comment */
          .container
            padding: 10px
        `;

        const result = minifyContentSync(content, 'styles.sass');
        expect(result.length).toBeLessThan(content.length);
      });

      it('should minify LESS files', () => {
        const content = `
          /* LESS styles */
          @color: blue;
          .btn { color: @color; }
        `;

        const result = minifyContentSync(content, 'styles.less');
        expect(result.length).toBeLessThan(content.length);
      });
    });

    describe('HTML/XML minification', () => {
      it('should minify HTML files', () => {
        const content = `
          <!DOCTYPE html>
          <!-- Main page -->
          <html>
            <head>
              <title>Test</title>
            </head>
            <body>
              <div>   Hello   </div>
            </body>
          </html>
        `;

        const result = minifyContentSync(content, 'index.html');

        expect(result).not.toContain('<!-- Main page -->');
        expect(result.length).toBeLessThan(content.length);
      });

      it('should minify HTM files', () => {
        const content = `
          <!-- Comment -->
          <div>Test</div>
        `;

        const result = minifyContentSync(content, 'page.htm');
        expect(result).not.toContain('<!-- Comment -->');
      });

      it('should minify XML files', () => {
        const content = `
          <?xml version="1.0"?>
          <!-- Configuration -->
          <config>
            <setting>value</setting>
          </config>
        `;

        const result = minifyContentSync(content, 'config.xml');
        expect(result.length).toBeLessThan(content.length);
      });
    });

    describe('Markdown minification', () => {
      it('should minify Markdown files', () => {
        const content = `# Title


Some text here


Another paragraph  `;

        const result = minifyContentSync(content, 'README.md');

        expect(result).not.toMatch(/\n{3,}/);
        expect(result).not.toMatch(/ {2}$/m);
      });

      it('should minify .markdown files', () => {
        const content = `# Test


Content`;

        const result = minifyContentSync(content, 'test.markdown');
        expect(result).not.toMatch(/\n{3,}/);
      });
    });

    describe('Conservative minification (indentation-sensitive)', () => {
      it('should conservatively minify Python files', () => {
        const content = `# Comment
def foo():
    return "bar"


x = foo()`;

        const result = minifyContentSync(content, 'script.py');

        expect(result).toContain('    return');
        expect(result).not.toMatch(/\n{3,}/);
      });

      it('should minify Ruby files (aggressive - not indentation-sensitive)', () => {
        const content = `# Ruby code
def hello
  puts "Hello"
end


hello()`;

        const result = minifyContentSync(content, 'script.rb');
        expect(result).not.toContain('# Ruby code');
        expect(result.length).toBeLessThan(content.length);
      });

      it('should conservatively minify Shell files', () => {
        const content = `#!/bin/bash
# Script
echo "Hello"


exit 0`;

        const result = minifyContentSync(content, 'script.sh');
        expect(result).not.toMatch(/\n{3,}/);
      });

      it('should conservatively minify Bash files', () => {
        const content = `#!/bin/bash
echo "test"  `;

        const result = minifyContentSync(content, 'script.bash');
        expect(result).not.toMatch(/ {2}$/m);
      });

      it('should conservatively minify YAML files', () => {
        const content = `name: test
version: 1.0


dependencies:
  - foo
  - bar`;

        const result = minifyContentSync(content, 'config.yaml');
        expect(result).not.toMatch(/\n{3,}/);
      });

      it('should conservatively minify YML files', () => {
        const content = `key: value  
another: value`;

        const result = minifyContentSync(content, 'config.yml');
        expect(result).not.toMatch(/ {2}$/m);
      });
    });

    describe('General minification (unknown types)', () => {
      it('should apply general minification to unknown file types', () => {
        const content = `Some content here


With extra blank lines

And trailing spaces  `;

        const result = minifyContentSync(content, 'file.unknown');

        expect(result).not.toMatch(/\n{3,}/);
        expect(result).not.toMatch(/ {2}$/m);
      });

      it('should handle files without extension', () => {
        const content = `Content


More content`;

        const result = minifyContentSync(content, 'Makefile');
        expect(result).not.toMatch(/\n{3,}/);
      });
    });

    describe('Error handling', () => {
      it('should return original content on error', () => {
        const content = 'test content';

        const result = minifyContentSync(content, 'test.ts');

        expect(typeof result).toBe('string');
      });
    });

    describe('File types without comments config', () => {
      it('should handle CSV files conservatively without removing comments', () => {
        const csvContent = `name,age,city
John,30,NYC


Jane,25,LA`;

        const result = minifyContentSync(csvContent, 'data.csv');

        expect(result).not.toMatch(/\n{3,}/);
        expect(result).toContain('name,age,city');
      });

      it('should handle TXT files with general strategy', () => {
        const txtContent = `Plain text content

With blank lines


And more content    `;

        const result = minifyContentSync(txtContent, 'notes.txt');

        expect(result).not.toMatch(/\n{3,}/);
        expect(result).not.toMatch(/ {2,}$/m);
      });

      it('should handle LOG files with general strategy', () => {
        const logContent = `[INFO] Message\n\n[ERROR] Error`;

        const result = minifyContentSync(logContent, 'app.log');

        expect(result).toContain('[INFO]');
        expect(result).toContain('[ERROR]');
      });
    });

    describe('SVG minification', () => {
      it('should minify SVG files with HTML strategy', () => {
        const svgContent = `<svg>
  <!-- comment -->
  <rect width="100" height="100" />
</svg>`;

        const result = minifyContentSync(svgContent, 'icon.svg');

        expect(result).not.toContain('<!-- comment -->');
        expect(result).toContain('<svg>');
      });
    });

    describe('XML minification', () => {
      it('should minify XML files with HTML strategy', () => {
        const xmlContent = `<?xml version="1.0"?>
<!-- config comment -->
<config>
  <setting>value</setting>
</config>`;

        const result = minifyContentSync(xmlContent, 'config.xml');

        expect(result).not.toContain('<!-- config comment -->');
        expect(result).toContain('<config>');
      });
    });
  });
});

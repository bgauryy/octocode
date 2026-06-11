import { describe, it, expect } from 'vitest';
import { minifyContent } from '@octocodeai/octocode-minifier';

const GO = [
  '// stops the machine',
  'func (m *Machine) Stop() error {',
  '\treturn nil',
  '}',
  'func (m *Machine) Start() error {',
  '\treturn nil',
  '}',
].join('\n');

describe('minifier keeps C-family / scripting code readable', () => {
  it('Go: conservative — preserves newlines, never glues } to the next func', async () => {
    const r = await minifyContent(GO, 'machine.go');
    expect(r.failed).toBe(false);
    expect(r.type).toBe('conservative');
    expect(r.content).toContain('\n');
    expect(r.content).not.toMatch(/\}func/);
    expect(r.content).not.toContain('// stops the machine');
  });

  const readableCases: Array<{
    extension: string;
    source: string;
    expectedCode: string;
    removedComment: string;
  }> = [
    {
      extension: 'java',
      source: '// hidden java\nclass App {\n  void run() {}\n}\n',
      expectedCode: 'class App',
      removedComment: 'hidden java',
    },
    {
      extension: 'c',
      source:
        '/* hidden c */\n#include <stdio.h>\nint main() {\n  return 0;\n}\n',
      expectedCode: '#include <stdio.h>',
      removedComment: 'hidden c',
    },
    {
      extension: 'cpp',
      source: '// hidden cpp\n#include <vector>\nstd::vector<int> values;\n',
      expectedCode: '#include <vector>',
      removedComment: 'hidden cpp',
    },
    {
      extension: 'cs',
      source: '// hidden cs\nusing System;\nclass App {}\n',
      expectedCode: 'using System;',
      removedComment: 'hidden cs',
    },
    {
      extension: 'rust',
      source: '// hidden rust\nfn main() {\n  println!("hi");\n}\n',
      expectedCode: 'fn main()',
      removedComment: 'hidden rust',
    },
    {
      extension: 'rs',
      source: '/* hidden rs */\nfn main() {\n  println!("hi");\n}\n',
      expectedCode: 'println!("hi");',
      removedComment: 'hidden rs',
    },
    {
      extension: 'swift',
      source: '// hidden swift\nimport Foundation\nfunc run() {}\n',
      expectedCode: 'import Foundation',
      removedComment: 'hidden swift',
    },
    {
      extension: 'kotlin',
      source: '// hidden kotlin\nfun main() {\n  println("hi")\n}\n',
      expectedCode: 'fun main()',
      removedComment: 'hidden kotlin',
    },
    {
      extension: 'scala',
      source: '// hidden scala\nobject App {\n  def run(): Unit = ()\n}\n',
      expectedCode: 'object App',
      removedComment: 'hidden scala',
    },
    {
      extension: 'dart',
      source: '// hidden dart\nvoid main() {\n  print("hi");\n}\n',
      expectedCode: 'void main()',
      removedComment: 'hidden dart',
    },
    {
      extension: 'php',
      source: '<?php\n# hidden php\nfunction run() {\n  return true;\n}\n',
      expectedCode: 'function run()',
      removedComment: 'hidden php',
    },
    {
      extension: 'rb',
      source: '# hidden ruby\ndef run\n  puts "hi"\nend\n',
      expectedCode: 'def run',
      removedComment: 'hidden ruby',
    },
    {
      extension: 'perl',
      source: '# hidden perl\nsub run {\n  print "hi";\n}\n',
      expectedCode: 'sub run',
      removedComment: 'hidden perl',
    },
  ];

  it.each(readableCases)(
    '$extension uses newline-preserving conservative minification',
    async ({ extension, source, expectedCode, removedComment }) => {
      const r = await minifyContent(source, `x.${extension}`);

      expect(r.type).toBe('conservative');
      expect(r.content).toContain('\n');
      expect(r.content).toContain(expectedCode);
      expect(r.content).not.toContain(removedComment);
      expect(r.content).not.toMatch(/\}\s*(?:class|fn|func|def|sub|void)/);
    }
  );
});

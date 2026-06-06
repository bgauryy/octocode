import { describe, it, expect } from 'vitest';
import { minifyContent } from '../../../src/utils/minifier/minifier.js';

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

  it.each([
    'java',
    'c',
    'cpp',
    'cs',
    'rust',
    'rs',
    'swift',
    'kotlin',
    'scala',
    'dart',
    'php',
    'rb',
    'perl',
  ])('%s uses the newline-preserving conservative strategy', async ext => {
    const src = `a {\n  doThing()\n}\n\n\n\nb()\n`;
    const r = await minifyContent(src, `x.${ext}`);
    expect(r.type).toBe('conservative');
    expect(r.content).toContain('\n');
  });
});

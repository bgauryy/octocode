// Partial reasons a local fixture can cause, by size or by shape of the file.
const scene = (id, tool) => (query, reason, files, more = {}) => ({ id: `reason.${id}`, tool, query, reasons: [reason], files, ...more });

export const REASON_SCENARIOS = [
  scene('binaryQuit', 'localSearch')({ path: '$DIR', matchString: ['needle'] }, 'binaryQuit', { 'a.txt': Buffer.concat([Buffer.from('needle\n'), Buffer.from([0]), Buffer.from('tail\n')]) }),
  scene('regexMatchLimit', 'localFetch')({ path: '$DIR/dense.txt', matchString: ['x'], regexEngine: 'rust' }, 'regexMatchLimit', { 'dense.txt': `${'x'.repeat(1000)}\n`.repeat(101) }),
  scene('filesSkipped', 'astSearch')({ path: '$DIR', operation: 'match', parser: 'typescript', rule: { pattern: 'foo($A)' } }, 'filesSkipped', { 'a.ts': 'foo(1);\n', 'bin.ts': Buffer.from([0xff, 0x66, 0x6f, 0x6f, 0x28, 0xfe]) }),
  scene('parseRecovered', 'astSearch')({ path: '$DIR/a.ts', operation: 'match', parser: 'typescript', rule: { pattern: 'foo($A)' } }, 'parseRecovered', { 'a.ts': 'foo(1);\nclass { {{{ broken\n' }),
];

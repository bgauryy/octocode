// Partial reasons a local fixture can cause, by size or by shape of the file.
const scene = (id, tool) => (query, reason, files, more = {}) => ({ id: `reason.${id}`, tool, query, reasons: [reason], files, ...more });

export const REASON_SCENARIOS = [
  scene('binaryQuit', 'localSearch')({ path: '$DIR', matchString: ['needle'] }, 'binaryQuit', { 'a.txt': Buffer.concat([Buffer.from('needle\n'), Buffer.from([0]), Buffer.from('tail\n')]) }),
  scene('regexMatchLimit', 'localFetch')({ path: '$DIR/dense.txt', matchString: ['x'], regexEngine: 'rust' }, 'regexMatchLimit', { 'dense.txt': `${'x'.repeat(1000)}\n`.repeat(101) }),
  scene('filesSkipped', 'astSearch')({ path: '$DIR', operation: 'match', parser: 'typescript', rule: { pattern: 'foo($A)' } }, 'filesSkipped', { 'a.ts': 'foo(1);\n', 'bin.ts': Buffer.from([0xff, 0x66, 0x6f, 0x6f, 0x28, 0xfe]) }),
  // Counted only when the parse error sits at the query's anchor (engine test match_tests/grammars.rs).
  scene('parseRecovered', 'astSearch')({ path: '$DIR', operation: 'match', parser: 'cpp', rule: { pattern: 'JSON_ASSERT($$$A)' } }, 'parseRecovered', { 'garbled.cpp': "class lexer {\n    JSON_HEDLEY_NON_NULL(2)\n    token_type scan_literal(const char_type* literal_text, const std::size_t length,\n                            token_type return_type)\n    {\n        JSON_ASSERT(char_traits<char_type>::to_char_type(current) == literal_text[0]);\n        return return_type;\n    }\n};\n", 'clean.cpp': 'int f() { JSON_ASSERT(1); return 2; }\n', 'local.cpp': 'struct S {\n    JSON_HEDLEY_NON_NULL(2)\n    int scan(const char* a, int b)\n    {\n        JSON_ASSERT(a != nullptr);\n        return b;\n    }\n};\n' }),
];

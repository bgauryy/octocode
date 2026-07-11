// Worked examples for lspGetSemantics, keyed by `type` — shown by
// showToolHelp() since the LSP tool's flat display fields don't capture the
// per-type field combinations (e.g. documentSymbols needs no symbolName).
export const LSP_TYPE_EXAMPLES: Array<[string, Record<string, unknown>]> = [
  [
    'definition — jump to declaration',
    {
      uri: '/path/to/file.ts',
      type: 'definition',
      symbolName: 'myFunction',
      lineHint: 42,
    },
  ],
  [
    'references — all usages',
    {
      uri: '/path/to/file.ts',
      type: 'references',
      symbolName: 'MyClass',
      lineHint: 10,
    },
  ],
  [
    'callers — who calls this function',
    {
      uri: '/path/to/file.ts',
      type: 'callers',
      symbolName: 'handleRequest',
      lineHint: 55,
    },
  ],
  [
    'callees — what this function calls',
    {
      uri: '/path/to/file.ts',
      type: 'callees',
      symbolName: 'handleRequest',
      lineHint: 55,
    },
  ],
  [
    'hover — type signature + docs',
    {
      uri: '/path/to/file.ts',
      type: 'hover',
      symbolName: 'myVar',
      lineHint: 20,
    },
  ],
  [
    'documentSymbols — file outline (no symbolName/lineHint needed)',
    { uri: '/path/to/file.ts', type: 'documentSymbols' },
  ],
  [
    'typeDefinition — where the type was declared',
    {
      uri: '/path/to/file.ts',
      type: 'typeDefinition',
      symbolName: 'myVar',
      lineHint: 20,
    },
  ],
  [
    'implementation — concrete impl of interface member',
    {
      uri: '/path/to/file.ts',
      type: 'implementation',
      symbolName: 'render',
      lineHint: 88,
    },
  ],
];

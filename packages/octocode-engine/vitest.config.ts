import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      // Instrument the real TS orchestration layer that tools-core imports
      // (the ./lsp/* and ./security/* subpath exports), plus the loader shims
      // and build scripts. Previously this listed only index.js/index.mjs/scripts,
      // which covered ~trivial loaders and excluded all of src/ — so coverage
      // reported near-100% on noise and 0% on the real 7.4k LOC TS layer.
      include: ['src/**/*.ts', 'index.js', 'index.cjs', 'scripts/**/*.cjs'],
      exclude: [
        'target/**',
        'coverage/**',
        'node_modules/**',
        '*.node',
        '*.d.ts',
      ],
      reporter: ['text'],
    },
  },
})

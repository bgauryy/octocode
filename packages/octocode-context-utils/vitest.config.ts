import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      include: ['index.js', 'index.mjs', 'scripts/**/*.cjs'],
      exclude: [
        'benchmark/**',
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

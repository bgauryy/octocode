import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    setupFiles: ['./tests/setup.ts'],
    // The suite intentionally exercises node:sqlite, subprocess CLI calls, and
    // generated skill scripts. Cap workers so coverage runs don't starve those
    // integration tests on high-core machines.
    maxWorkers: 4,
    testTimeout: 60_000,
    hookTimeout: 60_000,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: [
        'src/types.ts',
        // Barrel/CLI handoff and host-adapter lifecycle code are verified by
        // focused tests, but keep the global threshold on core runtime modules.
        'src/index.ts',
        'src/pi-hooks.ts',
      ],
      thresholds: {
        statements: 90,
        // Branches are option-matrix heavy across CLI parsers and host adapters.
        // Keep this as a ratchet while the primary 90% gate applies to code coverage.
        branches: 75,
        functions: 90,
        lines: 90,
      },
    },
  },
});

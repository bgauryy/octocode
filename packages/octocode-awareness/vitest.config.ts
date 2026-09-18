import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    setupFiles: [
      '../../test-utils/external-effects-guard.ts',
      './tests/setup.ts',
    ],
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
        // Barrel/CLI handoff and host-adapter lifecycle code are verified by
        // focused tests, but keep the global threshold on core runtime modules.
        'src/index.ts',
        'src/pi-hooks.ts',
        // Type-only modules — all definitions are erased at runtime.
        'src/types/**',
      ],
      thresholds: {
        // Ratchet after validating the shared API, CLI, stores and host adapters.
        statements: 90,
        branches: 80,
        functions: 95.3,
        lines: 93.4,
      },
    },
  },
});

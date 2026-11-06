import { defineConfig } from 'vitest/config';
import base from './vitest.config';

// The built launcher against the real native binary; each suite skips when either is absent.
export default defineConfig({
  test: {
    ...base.test,
    include: [
      'tests/e2e/launcher.e2e.test.ts',
      'tests/e2e/config-view.e2e.test.ts',
    ],
    exclude: [],
  },
});

import { vi, beforeEach } from 'vitest';

// Production defaults must not depend on a developer's global .octocoderc.
// Individual policy tests override this explicitly.
process.env.OCTOCODE_STORAGE_MODE = 'persistent';

vi.spyOn(process, 'exit').mockImplementation(code => {
  throw new Error(`process.exit(${code})`);
});

beforeEach(() => {
  vi.clearAllMocks();
});

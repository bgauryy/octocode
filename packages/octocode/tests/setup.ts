import { vi, beforeEach } from 'vitest';

vi.spyOn(process, 'exit').mockImplementation(code => {
  throw new Error(`process.exit(${code})`);
});

beforeEach(() => {
  vi.clearAllMocks();
});

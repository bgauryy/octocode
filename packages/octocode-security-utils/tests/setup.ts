import { vi } from 'vitest';

// Increase max listeners to avoid warnings in test environments
process.setMaxListeners(50);

// Mock child_process.spawn for security tests that need command execution
vi.mock('child_process', async importOriginal => {
  const actual = await importOriginal<typeof import('child_process')>();
  return {
    ...actual,
    spawn: vi.fn(),
  };
});

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  loadInquirer: vi.fn().mockResolvedValue(undefined),
  clearScreen: vi.fn(),
  printWelcome: vi.fn(),
  printGoodbye: vi.fn(),
  checkAndPrintEnvironmentWithLoader: vi.fn().mockResolvedValue({
    nodeInstalled: true,
    nodeVersion: '22.0.0',
    gitInstalled: true,
  }),
  hasEnvironmentIssues: vi.fn().mockReturnValue(false),
  printNodeDoctorHint: vi.fn(),
  runMenuLoop: vi.fn().mockResolvedValue(undefined),
  Spinner: class {
    start() {
      return this;
    }
    clear() {}
  },
}));

vi.mock('../../src/utils/prompts.js', () => ({
  loadInquirer: mocks.loadInquirer,
}));

vi.mock('../../src/utils/platform.js', () => ({
  clearScreen: mocks.clearScreen,
}));

vi.mock('../../src/ui/header.js', () => ({
  printWelcome: mocks.printWelcome,
  printGoodbye: mocks.printGoodbye,
}));

vi.mock('../../src/ui/install/environment.js', () => ({
  checkAndPrintEnvironmentWithLoader: mocks.checkAndPrintEnvironmentWithLoader,
  hasEnvironmentIssues: mocks.hasEnvironmentIssues,
  printNodeDoctorHint: mocks.printNodeDoctorHint,
}));

vi.mock('../../src/ui/menu/main-menu.js', () => ({
  runMenuLoop: mocks.runMenuLoop,
}));

vi.mock('../../src/utils/spinner.js', () => ({
  Spinner: mocks.Spinner,
}));

describe('runInteractiveMode', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    consoleSpy.mockRestore();
  });

  it('runs through the full interactive flow', async () => {
    const { runInteractiveMode } = await import('../../src/interactive.js');
    await runInteractiveMode();

    expect(mocks.loadInquirer).toHaveBeenCalledTimes(1);
    expect(mocks.clearScreen).toHaveBeenCalledTimes(1);
    expect(mocks.printWelcome).toHaveBeenCalledTimes(1);
    expect(mocks.checkAndPrintEnvironmentWithLoader).toHaveBeenCalledTimes(1);
    expect(mocks.runMenuLoop).toHaveBeenCalledTimes(1);
  });

  it('shows doctor hint when env has issues', async () => {
    mocks.hasEnvironmentIssues.mockReturnValueOnce(true);

    const { runInteractiveMode } = await import('../../src/interactive.js');
    await runInteractiveMode();

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('node-doctor')
    );
    expect(mocks.runMenuLoop).toHaveBeenCalledTimes(1);
  });

  it('stops and shows error when Node.js is not installed', async () => {
    mocks.checkAndPrintEnvironmentWithLoader.mockResolvedValueOnce({
      nodeInstalled: false,
    });

    const { runInteractiveMode } = await import('../../src/interactive.js');
    await runInteractiveMode();

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Node.js is required')
    );
    expect(mocks.printNodeDoctorHint).toHaveBeenCalledTimes(1);
    expect(mocks.printGoodbye).toHaveBeenCalledTimes(1);
    expect(mocks.runMenuLoop).not.toHaveBeenCalled();
  });
});

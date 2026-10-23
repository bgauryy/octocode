import type { Checkpoints } from '../src/files/checkpoint.js';

/** One change through the checkpoint protocol the file tool uses. */
export async function change(checkpoints: Checkpoints, file: string, apply: () => void): Promise<void> {
  const capture = await checkpoints.capture(file);
  apply();
  await checkpoints.settle(file, capture, true);
}

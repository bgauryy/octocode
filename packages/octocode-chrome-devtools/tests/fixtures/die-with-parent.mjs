// Node args that make a spawned fixture server exit with its parent. Spawn it with stdin 'pipe'.
import { fileURLToPath } from 'node:url';

export const DIE_WITH_PARENT = [
  '-r',
  fileURLToPath(new URL('./die-with-parent.cjs', import.meta.url)),
];

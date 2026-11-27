import { cpSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

export function copyPackage(source, destination) {
  const manifest = JSON.parse(
    readFileSync(join(source, 'package.json'), 'utf8')
  );
  for (const file of ['package.json', ...manifest.files]) {
    cpSync(join(source, file), join(destination, file), { recursive: true });
  }
}

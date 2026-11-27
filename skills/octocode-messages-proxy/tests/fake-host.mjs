import { writeFileSync } from 'node:fs';
import path from 'node:path';

/**
 * Installs fake `ps` and `lsof` in dir. The process that runs `ps` (the launcher) becomes a child of selfParent;
 * rows are [pid, ppid, executable, command line]; cwd maps a pid to its working directory.
 */
export function fakeProcesses(dir, { selfParent = 1, rows = [], cwd = {} } = {}) {
  const table = JSON.stringify({ selfParent, rows, cwd });
  const script = (body) => `#!/usr/bin/env python3\nimport json, os, sys\ntable = json.loads(${JSON.stringify(table)})\n${body}`;
  writeFileSync(path.join(dir, 'ps'), script(`
field = 2 if 'comm=' in sys.argv[-1] else 3
for row in [[os.getppid(), table['selfParent'], 'python3', 'python3 relay.py'], *table['rows']]:
    print(row[0], row[1], row[field])
`), { mode: 0o700 });
  writeFileSync(path.join(dir, 'lsof'), script(`
for pid in sys.argv[sys.argv.index('-p') + 1].split(','):
    if pid in table['cwd']:
        print('p' + pid)
        print('n' + table['cwd'][pid])
`), { mode: 0o700 });
}

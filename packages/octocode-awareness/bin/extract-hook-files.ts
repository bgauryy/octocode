/**
 * extract-hook-files.ts — Extract file paths from a hook JSON payload (stdin).
 *
 * Handles Claude-style tool_input payloads, Cursor flat file_path payloads,
 * Pi tool events (`input`/`args`), and Codex apply_patch command strings.
 * Prints one path per line, deduplicated.
 * Exits 0 on any error (fail-open).
 *
 * Compiled to dist/bin/extract-hook-files.js.
 */

const USAGE = `usage: extract-hook-files < hook-payload.json

Reads a hook JSON payload from stdin and prints one deduplicated file path per line.
Supports Claude tool_input, Cursor file_path, Pi input/args, and Codex apply_patch command payloads.
`;

if (process.argv.includes('--help') || process.argv.includes('-h')) {
  process.stdout.write(USAGE);
  process.exit(0);
}

let raw = '';
process.stdin.on('data', (chunk: Buffer | string) => { raw += String(chunk); });
process.stdin.on('end', () => {
  try {
    let data: unknown;
    try {
      data = JSON.parse(raw);
    } catch {
      data = raw;
    }
    const root = data !== null && typeof data === 'object'
      ? data as Record<string, unknown>
      : {} as Record<string, unknown>;
    const toolInput = root.tool_input ?? root.input ?? root.args ?? data;
    const ti = toolInput !== null && typeof toolInput === 'object'
      ? (toolInput as Record<string, unknown>)
      : {} as Record<string, unknown>;

    const paths: string[] = [];

    function add(value: unknown): void {
      if (typeof value === 'string' && value.trim()) {
        paths.push(value.trim());
      } else if (Array.isArray(value)) {
        for (const item of value) add(item);
      }
    }

    function addTargets(source: Record<string, unknown>): void {
      add(source['file_path']);
      add(source['path']);
      add(source['filePath']);
      add(source['paths']);
      add(source['file_paths']);
      add(source['filePaths']);

      const queries = source['queries'];
      if (Array.isArray(queries)) {
        for (const query of queries) {
          if (!query || typeof query !== 'object') continue;
          addTargets(query as Record<string, unknown>);
        }
      }
    }

    addTargets(root);
    if (ti !== root) addTargets(ti);

    const command = typeof toolInput === 'string'
      ? toolInput
      : ti['command'] ?? root['command'] ?? ti['patch'] ?? root['patch'] ?? ti['text'] ?? root['text'] ?? ti['content'] ?? root['content'];
    if (typeof command === 'string') {
      for (const line of command.split('\n')) {
        const addUpdDel = line.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/);
        if (addUpdDel) { paths.push(addUpdDel[1]!.trim()); continue; }
        const moveTo = line.match(/^\*\*\* Move to: (.+)$/);
        if (moveTo) paths.push(moveTo[1]!.trim());
      }
    }

    const seen = new Set<string>();
    for (const p of paths) {
      if (p && !seen.has(p)) {
        seen.add(p);
        process.stdout.write(p + '\n');
      }
    }
  } catch {
    // Fail-open: parse error → print nothing, exit 0
  }
  process.exit(0);
});

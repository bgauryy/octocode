// Preload for node fixture servers: exit when the parent's stdin pipe closes (parent exit or kill).
process.stdin.on('end', () => process.exit(0)).on('error', () => process.exit(0));
process.stdin.unref();
process.stdin.resume();

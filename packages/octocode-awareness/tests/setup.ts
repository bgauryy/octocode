process.removeAllListeners('warning');
process.on('warning', (warning) => {
  if (warning?.name === 'ExperimentalWarning' && String(warning?.message).includes('SQLite')) {
    return;
  }
  // Preserve non-SQLite warnings; only the noisy node:sqlite experimental notice is filtered.
  process.stderr.write(`${warning?.stack ?? String(warning)}\n`);
});

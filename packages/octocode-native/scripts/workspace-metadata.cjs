'use strict';

const { execFileSync } = require('node:child_process');
const { join } = require('node:path');

function workspacePackages(root, { locked = true } = {}) {
  const args = ['metadata', '--manifest-path', join(root, 'Cargo.toml'), '--format-version', '1', '--no-deps', '--offline'];
  if (locked) args.push('--locked');
  const metadata = JSON.parse(execFileSync('cargo', args, { encoding: 'utf8' }));
  const members = new Set(metadata.workspace_members);
  return metadata.packages.filter(pkg => members.has(pkg.id));
}

module.exports = { workspacePackages };

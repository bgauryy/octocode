#!/usr/bin/env node
/**
 * Pre-pack guard for octocode-mcp.
 *
 * Native runtime assets (security, minifier, ripgrep) are now distributed as
 * npm optionalDependencies — no bundled .node files or rg binaries live inside
 * this package. Nothing to verify before packing.
 */
console.log('✓ octocode-mcp prepack: no bundled native assets to verify.');

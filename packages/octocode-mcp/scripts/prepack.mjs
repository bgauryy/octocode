#!/usr/bin/env node
/**
 * Pre-pack guard for octocode-mcp.
 *
 * Rust addons are distributed through npm optionalDependencies on
 * octocode-security and @octocodeai/octocode-context-utils.
 *
 * Ripgrep is resolved through the @vscode/ripgrep dependency for npm users.
 * Standalone Bun binaries still copy rg with build:bin:* scripts, but those
 * runtime files are not part of the octocode-mcp npm tarball.
 */
console.error('✓ octocode-mcp prepack: npm runtime assets are dependency-owned.');

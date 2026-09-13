# Octocode extension native runtime

`@octocodeai/octocode-extension-rust` is the native filesystem and line-diff
boundary used by the Octocode Pi extension and Awareness local history.

It provides bounded snapshots, atomic file replacement, deletion receipts,
private directory creation, line diffs, Awareness evidence fingerprints, and
verified loose Git object reads. It is separate from the research engine and
does not depend on Pi, tools-core, or the MCP server.

Prebuilt addons are distributed through platform-specific optional packages.
Consumers should install the root package and allow its loader to select the
matching addon.

## Requirements

- Node.js 24.15.x
- A supported macOS, Linux, or Windows platform package

## Development

From the repository root:

```bash
yarn workspace @octocodeai/octocode-extension-rust build
yarn workspace @octocodeai/octocode-extension-rust test
yarn workspace @octocodeai/octocode-extension-rust lint
yarn workspace @octocodeai/octocode-extension-rust platforms:check
```

See [native boundary architecture](ARCHITECTURE.md). Filesystem safety and
durability constraints are part of that contract and should be reviewed before
adding a mutation API.

## License

MIT

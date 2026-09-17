# octocode-native — Publishing Guide

## What is this package?

A pure **Rust CLI binary** distributed as a native executable via npm's
`optionalDependencies` pattern (same as esbuild, Biome, SWC). No Node.js
required at runtime.

```
User types:  octocode search …
                    ↓
     bin/octocode.cjs  (Node shim — detects platform)
                    ↓
     @octocodeai/octocode-native-darwin-arm64  (pre-built Rust binary)
                    ↓
     target/aarch64-apple-darwin/release/octocode
```

---

## Package map

```
packages/octocode-native/
├── package.json                  ← coordinator: @octocodeai/octocode-native
├── Cargo.toml                    ← crate name: octocode-native  |  bin: octocode
├── bin/
│   ├── octocode.cjs              ← platform-selecting shim (Node)
│   └── octocode-regex-worker.cjs ← regex-worker shim
├── npm/
│   ├── darwin-arm64/             ← @octocodeai/octocode-native-darwin-arm64
│   ├── darwin-x64/               ← @octocodeai/octocode-native-darwin-x64
│   ├── linux-arm64-gnu/          ← @octocodeai/octocode-native-linux-arm64-gnu
│   ├── linux-x64-gnu/            ← @octocodeai/octocode-native-linux-x64-gnu
│   ├── linux-x64-musl/           ← @octocodeai/octocode-native-linux-x64-musl
│   ├── win32-x64-msvc/           ← @octocodeai/octocode-native-win32-x64-msvc
│   └── verify-binary.cjs         ← prepublishOnly guard (binary present + non-empty)
└── scripts/
    ├── copy-binaries.cjs         ← copies cargo output → npm/<platform>/
    └── check-platform-binaries.cjs ← preflight: all 6 platforms have binaries
```

### Name vs command — why they differ

| Thing | Value | Where set |
|---|---|---|
| Crate name (crates.io identity) | `octocode-native` | `Cargo.toml [package] name` |
| npm coordinator package | `@octocodeai/octocode-native` | `package.json name` |
| Binary on `$PATH` | `octocode` | `Cargo.toml [[bin]] name` |

`crates.io/octocode` is taken by an unrelated project (muvon). We publish as
`octocode-native` on crates.io. The binary produced is still named `octocode`
regardless of crate name — users just type `octocode`.

---

## Platform targets

| npm package suffix | Rust triple | OS / notes |
|---|---|---|
| `darwin-arm64` | `aarch64-apple-darwin` | macOS Apple Silicon |
| `darwin-x64` | `x86_64-apple-darwin` | macOS Intel |
| `linux-x64-gnu` | `x86_64-unknown-linux-gnu` | Linux x64 glibc (Ubuntu, Debian, …) |
| `linux-x64-musl` | `x86_64-unknown-linux-musl` | Linux x64 musl (Alpine, Docker scratch) |
| `linux-arm64-gnu` | `aarch64-unknown-linux-gnu` | Linux ARM64 glibc (AWS Graviton, …) |
| `win32-x64-msvc` | `x86_64-pc-windows-msvc` | Windows x64 |

The `bin/octocode.cjs` shim auto-detects musl via `/etc/alpine-release`. No
user configuration needed.

---

## Prerequisites

```sh
# Rust toolchain
rustup target add aarch64-apple-darwin          # macOS cross: Intel → ARM
rustup target add x86_64-apple-darwin           # macOS cross: ARM → Intel
rustup target add aarch64-unknown-linux-gnu
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-unknown-linux-musl
rustup target add x86_64-pc-windows-msvc

# npm auth
npm whoami   # must be bgauryy or org member

# cargo auth (for crates.io publish, optional)
cargo login
```

---

## Build

### Single platform (local dev)

```sh
# Dev build — current host only
yarn workspace @octocodeai/octocode-native build:dev

# Release build — current host only
yarn workspace @octocodeai/octocode-native build

# Then copy into npm/<platform>/ dir
node packages/octocode-native/scripts/copy-binaries.cjs darwin-arm64
```

### All platforms

```sh
# Builds all 6 targets + copies into npm/*/
yarn workspace @octocodeai/octocode-native build:all
```

> **Cross-compilation note:** `build:all` requires all target toolchains
> installed. On macOS you can cross-compile to `darwin-x64` and both Linux musl
> targets natively. For Linux ARM64 and Windows you need either CI runners
> (recommended) or `cross` / `cargo-zigbuild`.

### Verify all platform binaries are present

```sh
yarn workspace @octocodeai/octocode-native platforms:check
```

Output must show ✓ for all 12 binaries (2 per platform × 6 platforms).

---

## Publish — step by step

### 1. Bump versions

All 7 packages (coordinator + 6 platform packages) must share the same version.
Edit `packages/octocode-native/package.json` and all `npm/*/package.json` files:

```sh
# Example: bump to 1.0.0
node -e "
const fs = require('fs');
const pkgs = [
  'packages/octocode-native/package.json',
  'packages/octocode-native/npm/darwin-arm64/package.json',
  'packages/octocode-native/npm/darwin-x64/package.json',
  'packages/octocode-native/npm/linux-arm64-gnu/package.json',
  'packages/octocode-native/npm/linux-x64-gnu/package.json',
  'packages/octocode-native/npm/linux-x64-musl/package.json',
  'packages/octocode-native/npm/win32-x64-msvc/package.json',
];
const version = '1.0.0';
for (const p of pkgs) {
  const d = JSON.parse(fs.readFileSync(p, 'utf8'));
  d.version = version;
  if (d.optionalDependencies) {
    for (const k of Object.keys(d.optionalDependencies)) d.optionalDependencies[k] = version;
  }
  fs.writeFileSync(p, JSON.stringify(d, null, 2) + '\n');
  console.log('bumped', p);
}
"
```

Also bump `Cargo.toml`:

```toml
[package]
version = "1.0.0"
```

### 2. Build all platform binaries

```sh
yarn workspace @octocodeai/octocode-native build:all
yarn workspace @octocodeai/octocode-native platforms:check
```

All 12 binaries must be ✓.

### 3. Run preflight

```sh
node scripts/prepublish.mjs        # check for workspace: / file: protocols
yarn workspace @octocodeai/octocode-native lint:rust
yarn workspace @octocodeai/octocode-native test:rust
```

### 4. Publish platform packages FIRST

Each platform package must be on npm before the coordinator — because the
coordinator's `optionalDependencies` pins exact versions.

```sh
for platform in darwin-arm64 darwin-x64 linux-arm64-gnu linux-x64-gnu linux-x64-musl win32-x64-msvc; do
  echo "Publishing @octocodeai/octocode-native-$platform …"
  npm publish packages/octocode-native/npm/$platform --access public
done
```

Verify they landed:

```sh
npm view @octocodeai/octocode-native-darwin-arm64 version
npm view @octocodeai/octocode-native-darwin-x64 version
npm view @octocodeai/octocode-native-linux-x64-gnu version
npm view @octocodeai/octocode-native-linux-x64-musl version
npm view @octocodeai/octocode-native-linux-arm64-gnu version
npm view @octocodeai/octocode-native-win32-x64-msvc version
```

### 5. Publish coordinator package

```sh
npm publish packages/octocode-native --access public
```

### 6. Smoke test

```sh
# Fresh install — downloads binary for current platform
npx @octocodeai/octocode-native@latest --version

# Full install
npm install -g @octocodeai/octocode-native
octocode --version
octocode --help
```

---

## GitHub Actions CI workflow (recommended)

Create `.github/workflows/native-release.yml`:

```yaml
name: Native Release

on:
  push:
    tags:
      - 'native-v*'          # trigger: git tag native-v1.0.0

permissions:
  contents: write            # for GitHub Release assets

jobs:
  build:
    name: Build ${{ matrix.platform }}
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: darwin-arm64
            runner: macos-latest
            target: aarch64-apple-darwin
          - platform: darwin-x64
            runner: macos-latest
            target: x86_64-apple-darwin
          - platform: linux-x64-gnu
            runner: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - platform: linux-x64-musl
            runner: ubuntu-latest
            target: x86_64-unknown-linux-musl
          - platform: linux-arm64-gnu
            runner: ubuntu-latest
            target: aarch64-unknown-linux-gnu
          - platform: win32-x64-msvc
            runner: windows-latest
            target: x86_64-pc-windows-msvc

    steps:
      - uses: actions/checkout@v5

      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: packages/octocode-native

      # musl needs cross-linker
      - name: Install musl tools
        if: matrix.platform == 'linux-x64-musl'
        run: sudo apt-get install -y musl-tools

      # ARM64 cross-compilation on Linux
      - name: Install ARM64 cross tools
        if: matrix.platform == 'linux-arm64-gnu'
        run: |
          sudo apt-get install -y gcc-aarch64-linux-gnu
          echo 'CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc' >> $GITHUB_ENV

      - name: Build
        working-directory: packages/octocode-native
        run: cargo build --release --bins --no-default-features --target ${{ matrix.target }}

      - name: Copy binaries
        working-directory: packages/octocode-native
        run: node scripts/copy-binaries.cjs ${{ matrix.platform }}

      # Upload platform npm dir as artifact (for publish job)
      - uses: actions/upload-artifact@v4
        with:
          name: binaries-${{ matrix.platform }}
          path: packages/octocode-native/npm/${{ matrix.platform }}/
          if-no-files-found: error

  publish:
    name: Publish to npm
    needs: build
    runs-on: ubuntu-latest
    environment: npm-publish       # protect with env secrets

    steps:
      - uses: actions/checkout@v5

      - uses: actions/setup-node@v5
        with:
          node-version: '24'
          registry-url: 'https://registry.npmjs.org'

      # Download all platform binaries into their npm/ dirs
      - uses: actions/download-artifact@v4
        with:
          pattern: binaries-*
          path: packages/octocode-native/npm-artifacts/

      - name: Place binaries
        run: |
          for platform in darwin-arm64 darwin-x64 linux-arm64-gnu linux-x64-gnu linux-x64-musl win32-x64-msvc; do
            cp -r packages/octocode-native/npm-artifacts/binaries-$platform/. \
                  packages/octocode-native/npm/$platform/
          done

      - name: Verify all binaries
        working-directory: packages/octocode-native
        run: node scripts/check-platform-binaries.cjs

      - name: Publish platform packages
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
        run: |
          for platform in darwin-arm64 darwin-x64 linux-arm64-gnu linux-x64-gnu linux-x64-musl win32-x64-msvc; do
            npm publish packages/octocode-native/npm/$platform --access public
          done

      - name: Publish coordinator
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
        run: npm publish packages/octocode-native --access public

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            packages/octocode-native/npm/darwin-arm64/octocode
            packages/octocode-native/npm/darwin-x64/octocode
            packages/octocode-native/npm/linux-x64-gnu/octocode
            packages/octocode-native/npm/linux-x64-musl/octocode
            packages/octocode-native/npm/linux-arm64-gnu/octocode
            packages/octocode-native/npm/win32-x64-msvc/octocode.exe
```

Trigger:

```sh
git tag native-v1.0.0
git push origin native-v1.0.0
```

---

## Homebrew tap update (after GitHub Release)

The tap `bgauryy/homebrew-octocode` currently installs the **Node** CLI.
When native is promoted, replace the formula:

```sh
# In a checkout of https://github.com/bgauryy/homebrew-octocode
# Formula/octocode.rb should fetch GitHub Release archives instead of npm tarball

cat > Formula/octocode.rb << 'EOF'
class Octocode < Formula
  desc "Code research CLI — local, GitHub, AST, LSP, packages"
  homepage "https://octocode.ai"
  version "1.0.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/bgauryy/octocode/releases/download/native-v#{version}/octocode-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_REAL_SHA256"
    end
    on_intel do
      url "https://github.com/bgauryy/octocode/releases/download/native-v#{version}/octocode-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_REAL_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/bgauryy/octocode/releases/download/native-v#{version}/octocode-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_REAL_SHA256"
    end
    on_intel do
      url "https://github.com/bgauryy/octocode/releases/download/native-v#{version}/octocode-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_REAL_SHA256"
    end
  end

  def install
    bin.install "octocode"
    libexec.install "octocode-regex-worker"   # internal helper — not on PATH
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/octocode --version")
  end
end
EOF

brew style Formula/octocode.rb
brew install --build-from-source bgauryy/octocode/octocode
brew test octocode
git commit -am "octocode #{version}"
git push
```

---

## crates.io (optional, later)

Skip until `octocode-engine` is published to crates.io first (it's a `path`
dependency today).

When ready:
1. Publish `octocode-engine` to crates.io
2. Replace `path` dep with registry version in `Cargo.toml`
3. `cargo publish --manifest-path packages/octocode-native/Cargo.toml`
4. Users: `cargo install octocode-native` → binary on PATH: `octocode`

Do **not** try to claim `crates.io/octocode` — taken by muvon.

---

## What NOT to do

| ❌ Don't | Why |
|---|---|
| Publish platform packages AFTER coordinator | npm resolves optionalDeps at install time — missing platform pkg = broken install |
| Add `octocode-regex-worker` to Homebrew `bin.install` | It's an internal worker, not a user command |
| Build with `--features napi-addon` for the CLI binary | NAPI addon is the `.node` library build; CLI uses `--no-default-features` |
| `cargo install octocode` | Installs muvon's unrelated product |
| Run `npx <pkg>@latest` in CI | Hangs on install prompt — use `npx -y` or pre-install |
| Claim crates.io `octocode` | Already taken |

---

## Quick reference

```sh
# Build everything locally
yarn workspace @octocodeai/octocode-native build:all
yarn workspace @octocodeai/octocode-native platforms:check

# Preflight
node scripts/prepublish.mjs
yarn workspace @octocodeai/octocode-native lint:rust
yarn workspace @octocodeai/octocode-native test:rust

# Publish
for p in darwin-arm64 darwin-x64 linux-arm64-gnu linux-x64-gnu linux-x64-musl win32-x64-msvc; do
  npm publish packages/octocode-native/npm/$p --access public
done
npm publish packages/octocode-native --access public

# Smoke test
npx @octocodeai/octocode-native@latest --version
```

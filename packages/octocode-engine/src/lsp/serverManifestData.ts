// AUTO-DERIVED from serverManifest.json — the manifest as a TS module so the
// compiled dist needs no JSON copy step. Edit serverManifest.json then regenerate,
// or edit here directly (this file is the runtime source of truth).
import type { ManifestFile } from './serverManifest.js';

export const MANIFEST: ManifestFile = {
  "$comment": "Auto-download manifest for portable, toolchain-free language servers. Verified 2026-06-28 against live GitHub Releases. SHA-256 pinned per asset — download is refused if the checksum mismatches. Scope: Rust (rust-analyzer, gz, auto-download ready), C/C++ (clangd, zip, auto-download ready). Toolchain-coupled servers (gopls/go, jdtls/jre) are detect-and-instruct; pure-JS servers (typescript-language-server, pyright, yaml/json/html/css) are bundled npm deps. Markdown/MDX handled by the MINIFIER, NOT LSP.",
  "version": 1,
  "servers": {
    "rust-analyzer": {
      "languageId": "rust",
      "repo": "rust-lang/rust-analyzer",
      "releaseTag": "2026-06-22",
      "platforms": {
        "darwin-arm64": { "url": "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-aarch64-apple-darwin.gz", "archive": "gz", "binName": "rust-analyzer", "sha256": "c8cdf6d5e488752b907d5ee15e31768b59a78d992e9a54b9f9660e1bfdf39f27" },
        "darwin-x64": { "url": "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-x86_64-apple-darwin.gz", "archive": "gz", "binName": "rust-analyzer", "sha256": "feb7c170d2c1a2e4b8a88ac73f937eddb576828e3821b0a63ee0e64bd0bc9440" },
        "linux-x64": { "url": "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-x86_64-unknown-linux-gnu.gz", "archive": "gz", "binName": "rust-analyzer", "sha256": "9602ca5b24dcaa07a5a021274763bed367d8a32da9a226fe3e139de3306569cb" },
        "linux-x64-musl": { "url": "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-x86_64-unknown-linux-musl.gz", "archive": "gz", "binName": "rust-analyzer", "sha256": "fe1d7b0e9733f7a439e4b6f27b8c4cc7afd87ae28fc5b496eb8df31d674b78dd" },
        "linux-arm64": { "url": "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-aarch64-unknown-linux-gnu.gz", "archive": "gz", "binName": "rust-analyzer", "sha256": "bf65b0d4586f127ab11bf33476dd6aac82dad173946c5d3b1cede19d63ae85ed" },
        "win32-x64": { "url": "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-x86_64-pc-windows-msvc.zip", "archive": "zip", "binPath": "rust-analyzer.exe", "binName": "rust-analyzer.exe", "sha256": "6071dc5b28aa6d22c715f63c08d75b827c066be4ea866796587e52ed48b2922f" },
        "win32-arm64": { "url": "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-aarch64-pc-windows-msvc.zip", "archive": "zip", "binPath": "rust-analyzer.exe", "binName": "rust-analyzer.exe", "sha256": "30f873713ea3663db10999c23e95b74fe19968c893d5c0e9b8a896b31dbf8cf8" }
      }
    },
    "clangd": {
      "languageId": "cpp",
      "repo": "clangd/clangd",
      "releaseTag": "22.1.0",
      "launchArgs": [],
      "platforms": {
        "darwin-arm64": { "url": "https://github.com/clangd/clangd/releases/download/22.1.0/clangd-mac-22.1.0.zip", "archive": "zip", "binPath": "clangd_22.1.0/bin/clangd", "binName": "clangd", "sha256": "e31e271fe11f6dcd7cf87ca74be4a12788ff8ce5a0b07762583e335c058e939a" },
        "darwin-x64": { "url": "https://github.com/clangd/clangd/releases/download/22.1.0/clangd-mac-22.1.0.zip", "archive": "zip", "binPath": "clangd_22.1.0/bin/clangd", "binName": "clangd", "sha256": "e31e271fe11f6dcd7cf87ca74be4a12788ff8ce5a0b07762583e335c058e939a" },
        "linux-x64": { "url": "https://github.com/clangd/clangd/releases/download/22.1.0/clangd-linux-22.1.0.zip", "archive": "zip", "binPath": "clangd_22.1.0/bin/clangd", "binName": "clangd", "sha256": "71eddc5303da9a5bc5e8b509488b5b2c5acf45f20e33b8394e71a12a56d67198" },
        "win32-x64": { "url": "https://github.com/clangd/clangd/releases/download/22.1.0/clangd-windows-22.1.0.zip", "archive": "zip", "binPath": "clangd_22.1.0/bin/clangd.exe", "binName": "clangd.exe", "sha256": "c54e57dbff3ccc9e8352367ddb7030ad3f624073ec58c7477424e7919f578572" }
      },
      "unsupportedPlatforms": { "linux-arm64": "clangd publishes no linux-arm64 release asset; install via the system package manager." }
    }
  }
};

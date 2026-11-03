# Real language-server acceptance

Run from the repository root with the built CLI and MCP launchers:

```sh
OCTOCODE_LIVE=1 npx -y -p node@24.15.0 node packages/octocode-benchmark/local/e2e/harness/grammar-live.mjs
```

The runtime grammar catalog selects every owned extension. Every selected file must have an installed server; unavailable servers fail the run. Optional positional arguments select language names or extensions for diagnosis, for example `Go Java cs` or `sbt`. Unknown selections fail. `OCTOCODE_LIVE_KEEP=1` retains the temporary fixture directory named in the JSON result.

Results are written to `local/e2e/results/grammar-live.json`. Preserve that file before another run. It contains the native binary hash, contract fingerprint, full CLI/MCP responses and executable page walks, server discovery, fixture sources, and build command output. Definition and reference results must match independently derived source positions and canonical file identity exactly, including all declaration/reference rows and Unicode columns. A successful process or an empty semantic response does not satisfy the oracle.

## Provisioning

The suite launches genuine installed servers. It does not install them automatically or change the user's LSP configuration. On the Apple Silicon development host, the missing toolchains were installed through Homebrew, then Microsoft's supported .NET tool command:

```sh
HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_INSTALL_CLEANUP=1 brew install go gopls jdtls dotnet coursier metals asm-lsp scala-cli sbt
DOTNET_ROOT="$(brew --prefix dotnet)/libexec" DOTNET_GENERATE_ASPNET_CERTIFICATE=false DOTNET_CLI_TELEMETRY_OPTOUT=1 dotnet tool install --tool-path "$HOME/.local/share/octocode-e2e/bin" csharp-ls
```

The initial install used Go 1.27.3, gopls 0.24.0, JDTLS 1.63.0, .NET SDK 10.0.403, csharp-ls 0.28.0, Metals 1.6.7, asm-lsp 0.10.1, Scala CLI 1.18.0, and the sbt 2.0.12 launcher. Existing TypeScript/JavaScript, Python, Rust and clangd installations are also required. The actual selected command is recorded for every extension. Set `JAVA_HOME`, `DOTNET_ROOT`, and `PATH` for other installation layouts; the helper discovers the documented Apple Silicon Homebrew locations when these variables are unset and adds the private .NET tools directory to its child PATH. It does not edit shell profiles.

Official installation and configuration sources:

- [gopls](https://go.dev/gopls/)
- [Eclipse JDT Language Server](https://github.com/eclipse-jdtls/eclipse.jdt.ls)
- [csharp-ls](https://github.com/razzmatazz/csharp-language-server) and [.NET runtime lookup](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-environment-variables)
- [Metals editor integration](https://scalameta.org/metals/docs/integrations/new-editor/), [script projects](https://scalameta.org/metals/docs/features/scripts/), [sbt projects](https://scalameta.org/metals/docs/build-tools/sbt/), and [build-import configuration](https://scalameta.org/metals/docs/editors/user-configuration/)
- [Scala CLI IDE setup](https://scala-cli.virtuslab.org/docs/commands/setup-ide/)
- [asm-lsp project configuration](https://github.com/bergercookie/asm-lsp#optional-configure-via-asm-lsptoml)

## Fixture projects

All generated sources, build metadata and LSP configuration live in the suite's temporary directory. Java uses a correctly named public-class file and an Eclipse Java project, verified by `javac`. Go uses a module verified by `go test`. C# uses a .NET 10 project verified by `dotnet build --disable-build-servers`. Scala source and script files are compiled with Scala 2.13.18 and receive Scala CLI's generated BSP metadata. The sbt file uses actual build-setting syntax, with a pinned sbt project and the sbt-metals plugin matching the installed Metals version. The temporary Metals configuration explicitly enables build import and the build tool's BSP server. Assembly uses a Git project and an explicit NASM/x86-64 asm-lsp configuration for all three owned extensions; assembler diagnostics are disabled because this suite checks symbol navigation.

The sbt fixture pins 1.12.15. During provisioning, Metals 1.6.7 with sbt 2.0.12 imported the same valid build but returned no definition or reference locations for its local helper. That external version combination remains a recorded compatibility limit; it is not counted as passing coverage.

The Rust Cargo project, JS/TS project metadata and CUDA host-only compiler flags keep the existing source fixtures usable by their actual servers. The shared grammar fixtures remain independent from these live-project adaptations. Brokers and their server processes are stopped before temporary files are removed. The JSON also retains provider version probes and Metals/JDTLS logs.

$ErrorActionPreference = 'Stop'
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($arch) {
    'X64' { $target = 'x86_64-pc-windows-msvc' }
    'Arm64' { $target = 'aarch64-pc-windows-msvc' }
    default { throw "Unsupported Windows architecture: $arch" }
}
$binary = Join-Path $PSScriptRoot "bin/$target/octocode-agents-communication.exe"
if (!(Test-Path $binary)) { throw "This source-only skill has no binary for $target. Install a built skill bundle." }
& $binary @args
exit $LASTEXITCODE

$ErrorActionPreference = 'Stop'
$entry = Join-Path $PSScriptRoot 'communication.py'
if (!(Test-Path $entry)) { throw 'Communication scripts missing. Reinstall the complete skill folder.' }
$python = if ($env:OCTOCODE_PYTHON) { $env:OCTOCODE_PYTHON } else { 'python' }
& $python -B $entry @args
exit $LASTEXITCODE

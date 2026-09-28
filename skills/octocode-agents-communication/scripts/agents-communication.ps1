$ErrorActionPreference = 'Stop'
$binary = Join-Path $PSScriptRoot 'octocode-agents-communication.exe'
if (!(Test-Path $binary)) { throw 'Communication executable missing. Build the skill with node src/build-skill.mjs.' }
& $binary @args
exit $LASTEXITCODE

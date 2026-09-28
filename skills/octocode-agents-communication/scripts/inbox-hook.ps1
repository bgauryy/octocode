$ErrorActionPreference = 'Stop'
& "$PSScriptRoot/agents-communication.ps1" hook @args
exit $LASTEXITCODE

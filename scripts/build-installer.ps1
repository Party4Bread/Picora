param([string]$Compiler = $env:ISCC_PATH)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (-not $Compiler) {
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { $Compiler = $command.Source }
    else { $Compiler = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
}
if (-not (Test-Path -LiteralPath $Compiler)) {
    throw 'Install Inno Setup 6 or pass -Compiler with the path to ISCC.exe.'
}
Push-Location $root
try {
    $metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
    $version = ($metadata.packages | Where-Object name -eq 'picora').version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "Unsupported installer version: $version" }
    python scripts/package.py
    if ($LASTEXITCODE -ne 0) { throw 'Desktop packaging failed' }
    & $Compiler "/DAppVersion=$version" packaging/windows.iss
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    $installer = Join-Path $root "dist\picora-$version-windows-x64-setup.exe"
    if (-not (Test-Path -LiteralPath $installer)) { throw 'Installer output missing' }
    Write-Output $installer
} finally { Pop-Location }

# Run only on a disposable Windows CI runner: installs and uninstalls Picora.
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) {
    throw 'Installer smoke testing requires a disposable GitHub Actions runner.'
}
$root = Split-Path $PSScriptRoot -Parent
$installers = @(Get-ChildItem "$root\dist\*-windows-x64-setup.exe")
if ($installers.Count -ne 1) { throw 'Expected exactly one Windows installer' }
$destination = Join-Path $env:RUNNER_TEMP 'Picora installer smoke'
$setupArgs = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', "/DIR=`"$destination`"", '/TASKS=')
for ($iteration = 0; $iteration -lt 2; $iteration++) {
    $process = Start-Process -FilePath $installers[0].FullName -ArgumentList $setupArgs -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Install/upgrade failed: $($process.ExitCode)" }
}
$installed = Join-Path $destination 'picora.exe'
if ((Get-FileHash $installed).Hash -ne (Get-FileHash "$root\target\release\picora.exe").Hash) {
    throw 'Installed executable differs from release build'
}
$uninstaller = Join-Path $destination 'unins000.exe'
$process = Start-Process -FilePath $uninstaller -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Uninstall failed: $($process.ExitCode)" }
if (Test-Path -LiteralPath $installed) { throw 'Uninstall left the executable behind' }
Write-Output 'Install, same-version upgrade, and uninstall passed.'

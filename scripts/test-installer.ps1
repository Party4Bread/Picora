# Run only on a disposable Windows CI runner: installs and uninstalls Picora.
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) {
    throw 'Installer smoke testing requires a disposable GitHub Actions runner.'
}
$root = Split-Path $PSScriptRoot -Parent
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $visualStudio) { throw 'Cannot locate Visual C++ tools for import check' }
$dumpbin = Get-ChildItem "$visualStudio\VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe" | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $dumpbin) { throw 'Cannot locate dumpbin' }
$imports = & $dumpbin.FullName /dependents "$root\target\release\picora.exe"
if ($LASTEXITCODE -ne 0) { throw 'Import inspection failed' }
if ($imports -match '(?i)(VCRUNTIME|MSVCP)\d.*\.dll') { throw 'Installer executable depends on VC++ Redistributable' }
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
# Exercise loader dependencies without requiring a graphical desktop on the runner.
$help = Start-Process -FilePath $installed -ArgumentList '--help' -WindowStyle Hidden -Wait -PassThru
if ($help.ExitCode -ne 0) { throw 'Installed executable could not start' }
# Model the running-app mutex; native viewport tests run on an interactive desktop.
$guard = [System.Threading.Mutex]::new($false, 'Local\Picora.Desktop')
try {
    $blocked = Start-Process -FilePath $uninstaller -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -WindowStyle Hidden -Wait -PassThru
    if ($blocked.ExitCode -eq 0 -or -not (Test-Path -LiteralPath $installed)) {
        throw 'Uninstaller did not protect the running-app mutex'
    }
} finally { $guard.Dispose() }
$process = Start-Process -FilePath $uninstaller -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Uninstall failed: $($process.ExitCode)" }
if (Test-Path -LiteralPath $installed) { throw 'Uninstall left the executable behind' }
Write-Output 'Install, same-version upgrade, and uninstall passed.'

# Builds the Windows tray app and its per-user installer. Requires Inno Setup 6+.
param([string] $Compiler = $env:ISCC)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
Push-Location $root
try {
    if (-not $Compiler) {
        $candidates = @(
            (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
            (Join-Path $env:ProgramFiles 'Inno Setup 7\ISCC.exe'),
            (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
            (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 7\ISCC.exe')
        )
        $Compiler = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if (-not $Compiler) {
            $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
            if ($command) { $Compiler = $command.Source }
        }
    }
    if (-not $Compiler -or -not (Test-Path -LiteralPath $Compiler)) {
        throw 'Install Inno Setup from https://jrsoftware.org/isinfo.php or pass -Compiler <path to ISCC.exe>.'
    }
    & bun scripts/cargo.mjs build -p likh --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'The app build failed.' }
    # Catch an accidental console build before anyone downloads it.
    $binary = [IO.File]::ReadAllBytes((Join-Path $root 'target\release\likh.exe'))
    $pe = [BitConverter]::ToInt32($binary, 60)
    if ([BitConverter]::ToUInt16($binary, $pe + 24 + 68) -ne 2) {
        throw 'likh.exe is a console program; the installer must contain a Windows GUI app.'
    }
    $metadata = & bun scripts/cargo.mjs metadata --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the app version.' }
    $version = (($metadata | ConvertFrom-Json).packages | Where-Object name -eq likh).version
    & $Compiler "/DAppVersion=$version" installer/windows.iss
    if ($LASTEXITCODE -ne 0) { throw 'The installer build failed.' }
    Write-Output (Join-Path $root 'target\installer\likh-setup-windows-x64.exe')
} finally {
    Pop-Location
}

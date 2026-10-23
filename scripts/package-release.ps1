param([Parameter(Mandatory=$true)][string]$Exe, [string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$exePath = (Resolve-Path -LiteralPath $Exe).Path
$manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
$match = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"')
if (!$match.Success) { throw 'Cannot read package version.' }
$version = $match.Groups[1].Value
$name = "english-companion-lite-v$version-windows-x64"
$output = if ($OutputDirectory) { [IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $root 'release-dist' }
$stage = Join-Path $output $name
$zip = Join-Path $output "$name.zip"
if ((Test-Path -LiteralPath $stage) -or (Test-Path -LiteralPath $zip)) {
    throw 'Release candidate already exists. Review it before creating another candidate.'
}
$bytes = [IO.File]::ReadAllBytes($exePath)
if ($bytes.Length -lt 256 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) { throw 'Not a PE executable.' }
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
if ($pe -lt 0 -or $pe + 26 -ge $bytes.Length -or [BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550 -or [BitConverter]::ToUInt16($bytes, $pe+4) -ne 0x8664) {
    throw 'Only Windows x64 PE executables may be packaged.'
}
New-Item -ItemType Directory -Path $stage -Force | Out-Null
Copy-Item -LiteralPath $exePath -Destination (Join-Path $stage 'english-companion-lite.exe')
foreach ($file in @('README.md','README_EN.md','LICENSE','CHANGELOG.md','TESTING.md','SECURITY.md','THIRD_PARTY_NOTICES.md','config.example.toml')) {
    Copy-Item -LiteralPath (Join-Path $root $file) -Destination $stage
}
Copy-Item -LiteralPath (Join-Path $root 'docs') -Destination $stage -Recurse
New-Item -ItemType Directory -Path (Join-Path $stage 'third-party') | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'third-party/licenses') -Destination (Join-Path $stage 'third-party') -Recurse
New-Item -ItemType Directory -Path (Join-Path $stage 'third-party/toolchain') | Out-Null
Get-ChildItem (Join-Path $root 'third-party/toolchain') -File | Where-Object Name -ne 'COPYRIGHT.html' | Copy-Item -Destination (Join-Path $stage 'third-party/toolchain')
$exeHash = (Get-FileHash (Join-Path $stage 'english-companion-lite.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
"$exeHash  english-companion-lite.exe" | Set-Content (Join-Path $stage 'SHA256SUMS.txt') -Encoding ascii
Compress-Archive -LiteralPath $stage -DestinationPath $zip -CompressionLevel Optimal
$zipHash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
"$zipHash  $name.zip" | Set-Content (Join-Path $output 'SHA256SUMS.txt') -Encoding ascii
Write-Output "Prepared: $zip"
Write-Output "SHA256: $zipHash"
Write-Output 'Local preparation only. No repository or Release was published.'

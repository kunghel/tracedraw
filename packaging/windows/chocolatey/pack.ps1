# Make TraceDraw's Chocolatey package for a release.
#   pack.ps1 -Version 0.47.0 -SetupDir <folder with the two setups> -OutDir dist
# The version and the setups' SHA-256 go into a copy of this folder,
# then `choco pack` makes tracedraw.<version>.nupkg in OutDir.
param(
  [Parameter(Mandatory = $true)][string]$Version,
  [Parameter(Mandatory = $true)][string]$SetupDir,
  [string]$OutDir = 'dist'
)
$ErrorActionPreference = 'Stop'

$x86 = Join-Path $SetupDir "tracedraw-$Version-windows-x86-setup.exe"
$x64 = Join-Path $SetupDir "tracedraw-$Version-windows-x86_64-setup.exe"
if (-not (Test-Path $x64)) { throw "Missing setup: $x64" }
# The 32-bit setup is left out when its build failed.
$sumX86 = if (Test-Path $x86) { (Get-FileHash $x86 -Algorithm SHA256).Hash.ToLowerInvariant() } else { '' }
$sumX64 = (Get-FileHash $x64 -Algorithm SHA256).Hash.ToLowerInvariant()

$work = Join-Path ([System.IO.Path]::GetTempPath()) "tracedraw-choco-$Version"
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
Copy-Item -Recurse $PSScriptRoot $work
Remove-Item (Join-Path $work 'pack.ps1')
foreach ($f in Get-ChildItem $work -Recurse -File -Include *.nuspec, *.ps1) {
  $text = Get-Content -Raw $f.FullName
  $text = $text.Replace('@VERSION@', $Version).Replace('@SHA256_X86@', $sumX86).Replace('@SHA256_X64@', $sumX64)
  Set-Content -NoNewline -Encoding utf8 $f.FullName $text
}
New-Item -ItemType Directory -Force $OutDir | Out-Null
choco pack (Join-Path $work 'tracedraw.nuspec') --outputdirectory $OutDir
if ($LASTEXITCODE -ne 0) { throw "choco pack failed ($LASTEXITCODE)" }

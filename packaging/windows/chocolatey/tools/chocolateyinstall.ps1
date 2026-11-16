$ErrorActionPreference = 'Stop'

# The release's setups (Inno Setup), checked by SHA-256. The release
# workflow (pack.ps1) fills in the version and the checksums.
$version = '@VERSION@'
$base = "https://github.com/tracedraw/tracedraw/releases/download/v$version"

$packageArgs = @{
  packageName    = $env:ChocolateyPackageName
  fileType       = 'exe'
  url64bit       = "$base/tracedraw-$version-windows-x86_64-setup.exe"
  checksum64     = '@SHA256_X64@'
  checksumType64 = 'sha256'
  silentArgs     = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /ALLUSERS'
  validExitCodes = @(0)
}

# The 32-bit setup, when the release has one (its build may fail on its
# own without holding back the release).
$sum32 = '@SHA256_X86@'
if ($sum32) {
  $packageArgs.url = "$base/tracedraw-$version-windows-x86-setup.exe"
  $packageArgs.checksum = $sum32
  $packageArgs.checksumType = 'sha256'
}

Install-ChocolateyPackage @packageArgs

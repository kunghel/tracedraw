$ErrorActionPreference = 'Stop'

# The setup's own uninstaller, found by its name in Apps and Features.
[array]$keys = Get-UninstallRegistryKey -SoftwareName 'TraceDraw*'
foreach ($key in $keys) {
  $file = ($key.UninstallString -replace '"', '')
  Uninstall-ChocolateyPackage -PackageName $env:ChocolateyPackageName `
    -FileType 'exe' -SilentArgs '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' `
    -File $file -ValidExitCodes @(0)
}

$ErrorActionPreference = "Stop"

$files = @(
  ".\scripts\install-windows.ps1",
  ".\scripts\install-wsl.ps1",
  ".\scripts\uninstall-windows.ps1",
  ".\scripts\uninstall-wsl.ps1",
  ".\scripts\preflight-windows.ps1",
  ".\scripts\verify-artifact-windows.ps1",
  ".\scripts\validate-installed-windows.ps1",
  ".\scripts\sign-windows.ps1",
  ".\scripts\verify-windows-signatures.ps1",
  ".\scripts\support-bundle-windows.ps1"
)

foreach ($file in $files) {
  $tokens = $null
  $errors = $null
  [System.Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$errors) | Out-Null
  if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Error "${file}: $($_.Message)" }
    exit 1
  }
}

Write-Host "verify_release_windows_ok"

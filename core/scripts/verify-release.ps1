$ErrorActionPreference = "Stop"

# Every PowerShell script must parse, and must be ASCII: Windows PowerShell 5.1
# reads a script without a BOM in the system code page, so any other character
# breaks on a non-English Windows.
foreach ($file in Get-ChildItem -Path $PSScriptRoot -Filter *.ps1) {
  $tokens = $null
  $errors = $null
  [System.Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$tokens, [ref]$errors) | Out-Null
  if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Host "$($file.Name): line $($_.Extent.StartLineNumber): $($_.Message)" }
    exit 1
  }
  if ([System.IO.File]::ReadAllBytes($file.FullName) | Where-Object { $_ -gt 127 } | Select-Object -First 1) {
    Write-Host "$($file.Name): contains non-ASCII bytes"
    exit 1
  }
  Write-Host "ok: $($file.Name)"
}

Write-Host "verify_release_windows_ok"

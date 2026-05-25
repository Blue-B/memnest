param(
  [Parameter(Mandatory = $true)]
  [string[]]$FilePath
)

$ErrorActionPreference = "Stop"

foreach ($file in $FilePath) {
  if (-not (Test-Path $file)) {
    throw "file not found: $file"
  }
  $signature = Get-AuthenticodeSignature -FilePath $file
  if ($signature.Status -ne "Valid") {
    throw "invalid signature for ${file}: $($signature.StatusMessage)"
  }
  Write-Host "valid signature: $file"
}

param(
  [Parameter(Mandatory = $true)]
  [string]$Artifact,
  [Parameter(Mandatory = $true)]
  [string]$Checksum
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $Artifact)) {
  throw "artifact not found: $Artifact"
}

if (-not (Test-Path $Checksum)) {
  throw "checksum file not found: $Checksum"
}

$expected = ((Get-Content $Checksum -Raw).Trim().Split(" ")[0]).ToLowerInvariant()
$actual = (Get-FileHash $Artifact -Algorithm SHA256).Hash.ToLowerInvariant()

if ($expected -ne $actual) {
  throw "checksum mismatch for ${Artifact}. Expected $expected but got $actual"
}

Write-Host "verify_artifact_windows_ok"

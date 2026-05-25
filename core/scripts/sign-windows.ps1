param(
  [Parameter(Mandatory = $true)]
  [string[]]$FilePath,
  [string]$CertificatePath = "",
  [string]$CertificatePassword = "",
  [string]$CertificateThumbprint = "",
  [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"

if (-not $CertificatePath -and -not $CertificateThumbprint) {
  throw "Pass -CertificatePath for a PFX file or -CertificateThumbprint for a certificate in the current user's certificate store."
}

$certificate = $null
if ($CertificatePath) {
  if (-not (Test-Path $CertificatePath)) {
    throw "certificate file not found: $CertificatePath"
  }
  $flags = [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::Exportable
  if ($CertificatePassword) {
    $certificate = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($CertificatePath, $CertificatePassword, $flags)
  }
  else {
    $certificate = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($CertificatePath)
  }
}
else {
  $certificate = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Thumbprint -eq $CertificateThumbprint } |
    Select-Object -First 1
  if (-not $certificate) {
    throw "certificate thumbprint not found in Cert:\CurrentUser\My: $CertificateThumbprint"
  }
}

foreach ($file in $FilePath) {
  if (-not (Test-Path $file)) {
    throw "file not found: $file"
  }
  $result = Set-AuthenticodeSignature -FilePath $file -Certificate $certificate -TimestampServer $TimestampUrl
  if ($result.Status -ne "Valid") {
    throw "signing failed for ${file}: $($result.StatusMessage)"
  }
  Write-Host "signed: $file"
}

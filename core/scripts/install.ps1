# One-line Windows installer:
#   irm https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.ps1 | iex
#
# Downloads a release zip, checks its SHA-256, unpacks it and runs the per-user
# install-windows.ps1 from it (no administrator rights). `irm | iex` cannot pass
# parameters; to pass install-windows.ps1 parameters use
#   & ([scriptblock]::Create((irm <same url>))) -Port 3222 -Autocontext
# The release comes from the environment:
#   $env:MEMNEST_VERSION       release tag such as v0.6.0 (default: latest)
#   $env:MEMNEST_RELEASE_BASE  download base URL (default: the GitHub release)
# The body is a script block so nothing leaks into the calling session.
# Keep this file ASCII: Windows PowerShell 5.1 reads a script without a BOM in
# the system code page.
& {
  $ErrorActionPreference = "Stop"
  $ProgressPreference = "SilentlyContinue"
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  $repo = "https://github.com/Blue-B/memnest"
  $target = "x86_64-pc-windows-msvc"
  if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64" -or $env:PROCESSOR_ARCHITEW6432) {
    throw "Memnest publishes a Windows archive for 64-bit x86 only (found $env:PROCESSOR_ARCHITECTURE). Build from source: $repo"
  }

  $version = $env:MEMNEST_VERSION
  if (-not $version -or $version -eq "latest") {
    $version = (Invoke-RestMethod "https://api.github.com/repos/Blue-B/memnest/releases/latest").tag_name
    if (-not $version) { throw "failed to determine the latest version" }
  }
  $base = $env:MEMNEST_RELEASE_BASE
  if (-not $base) { $base = "$repo/releases/download/$version" }

  $archive = "memnest-$version-$target.zip"
  $temp = Join-Path ([System.IO.Path]::GetTempPath()) "memnest-install-$([guid]::NewGuid().ToString('N'))"
  New-Item -ItemType Directory -Path $temp | Out-Null
  try {
    Write-Host "Installing Memnest $version for $target"
    try {
      Invoke-WebRequest -UseBasicParsing "$base/$archive" -OutFile (Join-Path $temp $archive)
    }
    catch {
      throw "could not download $base/$archive ($($_.Exception.Message)). Releases up to v0.5.1 have no Windows archive; pick a newer one with `$env:MEMNEST_VERSION or build from source: $repo"
    }
    Invoke-WebRequest -UseBasicParsing "$base/$archive.sha256" -OutFile (Join-Path $temp "$archive.sha256")

    $expected = ((Get-Content (Join-Path $temp "$archive.sha256") -Raw).Trim() -split "\s+")[0].ToLowerInvariant()
    $actual = (Get-FileHash (Join-Path $temp $archive) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($expected -ne $actual) {
      throw "checksum mismatch for $archive. Expected $expected but got $actual; nothing was installed."
    }

    $unpacked = Join-Path $temp "unpacked"
    Expand-Archive -Path (Join-Path $temp $archive) -DestinationPath $unpacked
    # Running the downloaded script as a block keeps this working when the
    # execution policy blocks script files; nothing machine-wide is changed.
    $installer = [scriptblock]::Create((Get-Content (Join-Path $unpacked "scripts\install-windows.ps1") -Raw))
    Push-Location $unpacked
    try { & $installer @args }
    finally { Pop-Location }
  }
  finally {
    Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
  }
} @args

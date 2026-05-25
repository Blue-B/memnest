param(
  [string]$Out = "palimpsest-support-$(Get-Date -Format yyyyMMdd-HHmmss).txt",
  [string]$ServiceName = "palimpsest",
  [int]$Port = 3111
)

$ErrorActionPreference = "Stop"

function Add-Section {
  param([string]$Name)
  Add-Content -Path $Out -Value ""
  Add-Content -Path $Out -Value "## $Name"
}

function Add-Command {
  param(
    [string]$Label,
    [scriptblock]$Command
  )
  Add-Content -Path $Out -Value ""
  Add-Content -Path $Out -Value "`$ $Label"
  try {
    & $Command 2>&1 | Out-String | Add-Content -Path $Out
  }
  catch {
    Add-Content -Path $Out -Value $_.Exception.Message
  }
}

Set-Content -Path $Out -Value "## Palimpsest Support Bundle"
Add-Content -Path $Out -Value "created_at=$(Get-Date -Format o)"
Add-Content -Path $Out -Value "health_url=http://127.0.0.1:$Port/health"

Add-Section "System"
Add-Command "Get-ComputerInfo" { Get-ComputerInfo | Select-Object OsName, OsVersion, CsSystemType, CsTotalPhysicalMemory }
Add-Command "Get-Date" { Get-Date -Format o }
Add-Command "Get-PSDrive" { Get-PSDrive -PSProvider FileSystem }

Add-Section "Binary"
Add-Command "palimpsest --version" { & "$env:ProgramData\Palimpsest\app\palimpsest.exe" --version }

Add-Section "Health"
Add-Command "Invoke-WebRequest health" { Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$Port/health" | Select-Object -ExpandProperty Content }

Add-Section "Service"
Add-Command "Get-Service" { Get-Service -Name $ServiceName }
Add-Command "Recent service logs" {
  Get-ChildItem "$env:ProgramData\Palimpsest\logs" -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 5 |
    ForEach-Object {
      "### $($_.FullName)"
      Get-Content $_.FullName -Tail 80
    }
}

Write-Host "support bundle written: $Out"

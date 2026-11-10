param(
  [string]$Out = "memnest-support-$(Get-Date -Format yyyyMMdd-HHmmss).txt",
  [string]$InstallDir = "$env:LOCALAPPDATA\memnest",
  [string]$DataDir = "$env:USERPROFILE\.memnest",
  [string]$TaskName = "Memnest",
  [int]$Port = 3111
)

# Collects diagnostics for the per-user install. No memory text is included.
# Keep this file ASCII (see install-windows.ps1).
$ErrorActionPreference = "Stop"
$Exe = Join-Path $InstallDir "bin\memnest.exe"

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

Set-Content -Path $Out -Value "## Memnest Support Bundle"
Add-Content -Path $Out -Value "created_at=$(Get-Date -Format o)"
Add-Content -Path $Out -Value "health_url=http://127.0.0.1:$Port/health"

Add-Section "System"
Add-Command "Get-ComputerInfo" { Get-ComputerInfo | Select-Object OsName, OsVersion, CsSystemType, CsTotalPhysicalMemory }
Add-Command "Get-Date" { Get-Date -Format o }
Add-Command "Get-PSDrive" { Get-PSDrive -PSProvider FileSystem }

Add-Section "Binary"
Add-Command "memnest --version" { & $Exe --version }
Add-Command "memnest status" { & $Exe --port $Port --data-dir $DataDir status }

Add-Section "Health"
Add-Command "Invoke-WebRequest health" { Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$Port/health" | Select-Object -ExpandProperty Content }

Add-Section "Scheduled tasks"
foreach ($name in $TaskName, "$TaskName Watch") {
  Add-Command "Get-ScheduledTask '$name'" {
    $info = Get-ScheduledTaskInfo -TaskName $name
    "state=$((Get-ScheduledTask -TaskName $name).State) last_run=$($info.LastRunTime) last_result=$($info.LastTaskResult)"
  }.GetNewClosure()
}
Add-Command "Recent logs" {
  Get-ChildItem (Join-Path $InstallDir "logs") -File -ErrorAction SilentlyContinue |
    ForEach-Object {
      "### $($_.FullName)"
      Get-Content $_.FullName -Tail 80
    }
}

Write-Host "support bundle written: $Out"

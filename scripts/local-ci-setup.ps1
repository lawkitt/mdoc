# Usage: powershell -ExecutionPolicy Bypass -File scripts\local-ci-setup.ps1 [-Uninstall] [-Root D:\ci]
#
# Local CI on Windows (ADR 0082). Makes the CI checkout (its own clone at
# D:\ci\mdoc), copies the runner from this repository to D:\ci\runner, writes
# D:\ci\runner.env, registers the "mdoc local CI" Task Scheduler task (at logon,
# this user, no window) that runs `scripts/local-ci.sh run` in Git Bash, and
# serves D:\ci\out to the tailnet at /ci with `tailscale serve`. Re-run it from
# an updated checkout to update the runner. -Uninstall stops and removes the
# task and the tailnet path and keeps D:\ci.
[CmdletBinding()]
param(
    [string] $Root = 'D:\ci',
    [string] $Repo = 'lawkitt/mdoc',
    [switch] $Uninstall
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$taskName = 'mdoc local CI'
$bash = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
$source = Split-Path -Parent $PSScriptRoot
$checkout = Join-Path $Root 'mdoc'
$out = Join-Path $Root 'out'
$state = Join-Path $Root 'state'
$runnerScripts = Join-Path $Root 'runner\scripts'
# D:\ci as Git Bash sees it: /d/ci.
$bashRoot = '/' + $Root.Substring(0, 1).ToLower() + $Root.Substring(2).Replace('\', '/')

function Stop-Runner {
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $taskName
    }
    # Stopping the task ends the runner's bash but not its job's native
    # processes (cargo, rustc, test binaries): end each runner bash's tree.
    Get-CimInstance Win32_Process -Filter "Name = 'bash.exe'" |
        Where-Object { $_.CommandLine -like '*local-ci.sh*run*' } |
        ForEach-Object { & taskkill.exe /F /T /PID $_.ProcessId 2>$null | Out-Null }
    $lock = Join-Path $state 'lock'
    if (Test-Path $lock) { Remove-Item -Recurse -Force $lock }
}

if ($Uninstall) {
    Stop-Runner
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
    & tailscale serve --https=443 --set-path=/ci off 2>$null | Out-Null
    Write-Output "Removed the '$taskName' task and the /ci tailnet path; $Root is kept."
    return
}

foreach ($tool in 'git', 'gh', 'just', 'cargo', 'tailscale') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool not found on PATH" }
}
if (-not (Test-Path $bash)) { throw "Git Bash not found at $bash" }
& gh auth status *> $null
if ($LASTEXITCODE -ne 0) { throw 'gh is not logged in' }
$dns = ((& tailscale status --self --peers=false --json) | ConvertFrom-Json).Self.DNSName.TrimEnd('.')
if (-not $dns) { throw "could not read this machine's tailnet name" }

New-Item -ItemType Directory -Force -Path $Root, $out, $state, $runnerScripts | Out-Null
if (-not (Test-Path (Join-Path $checkout '.git'))) {
    Write-Output "Cloning the CI checkout into $checkout"
    & git clone --quiet "https://github.com/$Repo.git" $checkout
    if ($LASTEXITCODE -ne 0) { throw 'git clone failed' }
}

# Stop first: the running runner reads its copy of local-ci.sh.
Stop-Runner
foreach ($file in 'local-ci.sh', 'lib.sh') {
    Copy-Item -Force (Join-Path $source "scripts\$file") $runnerScripts
}
$envFile = @(
    '# Written by scripts/local-ci-setup.ps1; re-run it to change.'
    "LOCAL_CI_URL=https://$dns/ci"
)
# LF line endings: bash sources this file.
[IO.File]::WriteAllText((Join-Path $Root 'runner.env'), ($envFile -join "`n") + "`n")

# conhost --headless keeps the task from opening a console window; the
# pinned tools in ~/.cargo/bin come first, as on the Mac.
$command = "LOCAL_CI_HOME=$bashRoot PATH=`$HOME/.cargo/bin:`$PATH exec bash $bashRoot/runner/scripts/local-ci.sh run"
$action = New-ScheduledTaskAction -Execute (Join-Path $env:WINDIR 'System32\conhost.exe') `
    -Argument "--headless `"$bash`" -c `"$command`""
$trigger = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
    -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -StartWhenAvailable
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings `
    -Principal $principal -Description 'mdoc local CI runner (ADR 0082): scripts/local-ci.sh run' -Force | Out-Null
Start-ScheduledTask -TaskName $taskName

& tailscale serve --bg --set-path /ci $out | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'tailscale serve failed' }

Write-Output "Local CI installed: local/windows, checkout $checkout, results https://$dns/ci/"
Write-Output "Check it in Git Bash with: LOCAL_CI_HOME=$bashRoot just local-ci status"

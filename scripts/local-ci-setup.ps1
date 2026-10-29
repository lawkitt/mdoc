# Usage: powershell -ExecutionPolicy Bypass -File scripts\local-ci-setup.ps1 [-Uninstall] [-Root D:\ci]
#
# Local CI on Windows (ADR 0082). Makes the CI checkout (its own clone at
# D:\ci\mdoc), copies the runner from this repository to D:\ci\runner, writes
# D:\ci\runner.env and registers two Task Scheduler tasks (at logon, this user,
# no window): "mdoc local CI" runs `scripts/local-ci.sh run` in Git Bash, and
# "mdoc local CI web" serves D:\ci\out on 127.0.0.1 with Python's http.server,
# which `tailscale serve` proxies to the tailnet at /ci (serving a folder
# directly needs a Windows admin). Re-run it from an updated checkout to update
# the runner. -Uninstall stops and removes the tasks and the tailnet path and
# keeps D:\ci.
[CmdletBinding()]
param(
    [string] $Root = 'D:\ci',
    [string] $Repo = 'lawkitt/mdoc',
    [int] $Port = 8787,
    [switch] $Uninstall
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$taskName = 'mdoc local CI'
$webTaskName = 'mdoc local CI web'
$bash = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
$source = Split-Path -Parent $PSScriptRoot
$checkout = Join-Path $Root 'mdoc'
$out = Join-Path $Root 'out'
$state = Join-Path $Root 'state'
$runnerScripts = Join-Path $Root 'runner\scripts'

# A Windows path as Git Bash sees it: D:\ci -> /d/ci.
function ConvertTo-BashPath([string] $Path) {
    '/' + $Path.Substring(0, 1).ToLower() + $Path.Substring(2).Replace('\', '/')
}
$bashRoot = ConvertTo-BashPath $Root

function Stop-Runner {
    foreach ($name in $taskName, $webTaskName) {
        if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
            Stop-ScheduledTask -TaskName $name
        }
    }
    # Stopping a task ends its bash but not the native processes under it (a
    # job's cargo, rustc and test binaries; the web server's python): end each
    # runner bash's tree and the web server.
    Get-CimInstance Win32_Process -Filter "Name = 'bash.exe'" |
        Where-Object { $_.CommandLine -like '*local-ci.sh*run*' } |
        ForEach-Object { & taskkill.exe /F /T /PID $_.ProcessId 2>$null | Out-Null }
    Get-CimInstance Win32_Process -Filter "Name = 'python.exe'" |
        Where-Object { $_.CommandLine -like "*http.server*--bind 127.0.0.1 $Port*" } |
        ForEach-Object { & taskkill.exe /F /T /PID $_.ProcessId 2>$null | Out-Null }
    $lock = Join-Path $state 'lock'
    if (Test-Path $lock) { Remove-Item -Recurse -Force $lock }
}

if ($Uninstall) {
    Stop-Runner
    foreach ($name in $taskName, $webTaskName) {
        if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
            Unregister-ScheduledTask -TaskName $name -Confirm:$false
        }
    }
    & tailscale serve --https=443 --set-path=/ci off 2>$null | Out-Null
    Write-Output "Removed the '$taskName' and '$webTaskName' tasks and the /ci tailnet path; $Root is kept."
    return
}

foreach ($tool in 'git', 'gh', 'just', 'cargo', 'tailscale') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool not found on PATH" }
}
if (-not (Test-Path $bash)) { throw "Git Bash not found at $bash" }
# A real Python, not the Microsoft Store stub in WindowsApps.
$python = Get-Command python.exe -All -ErrorAction SilentlyContinue |
    Where-Object { $_.Source -notlike '*\WindowsApps\*' } | Select-Object -First 1
if (-not $python) { throw 'python not found on PATH (the results server needs it)' }
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
    "LOCAL_CI_PORT=$Port"
)
# LF line endings: bash sources this file.
[IO.File]::WriteAllText((Join-Path $Root 'runner.env'), ($envFile -join "`n") + "`n")

# A Git Bash command as a task at logon, this user, restarted if it ends.
# conhost --headless keeps it from opening a console window, but writes to
# that console fail (which ends a `set -e` script), so each command sends its
# output to a file.
function Register-CiTask([string] $Name, [string] $Command, [string] $Description) {
    $action = New-ScheduledTaskAction -Execute (Join-Path $env:WINDIR 'System32\conhost.exe') `
        -Argument "--headless `"$bash`" -c `"$Command`""
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
        -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -StartWhenAvailable
    $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName $Name -Action $action -Trigger $trigger -Settings $settings `
        -Principal $principal -Description $Description -Force | Out-Null
    Start-ScheduledTask -TaskName $Name
}

# The pinned tools in ~/.cargo/bin come first, as on the Mac.
Register-CiTask $taskName `
    "LOCAL_CI_HOME=$bashRoot PATH=`$HOME/.cargo/bin:`$PATH exec bash $bashRoot/runner/scripts/local-ci.sh run >>$bashRoot/state/task.log 2>&1" `
    'mdoc local CI runner (ADR 0082): scripts/local-ci.sh run'
# Localhost only; tailscale serve is the way in from the tailnet.
Register-CiTask $webTaskName `
    "exec '$(ConvertTo-BashPath $python.Source)' -m http.server --bind 127.0.0.1 $Port --directory '$($out.Replace('\', '/'))' >/dev/null 2>&1" `
    "mdoc local CI results (ADR 0082): $out on 127.0.0.1:$Port"

& tailscale serve --bg --set-path /ci "http://127.0.0.1:$Port" | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'tailscale serve failed' }

Write-Output "Local CI installed: local/windows, checkout $checkout, results https://$dns/ci/"
Write-Output "Check it in Git Bash with: LOCAL_CI_HOME=$bashRoot just local-ci status"

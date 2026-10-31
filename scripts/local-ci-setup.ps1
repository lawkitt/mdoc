# Usage: powershell -ExecutionPolicy Bypass -File scripts\local-ci-setup.ps1 [-Uninstall] [-Root E:\ci]
#
# Local CI on Windows (ADR 0082). Makes the CI checkout (its own clone at
# E:\ci\mdoc, on the Dev Drive), copies the runner from this repository to
# E:\ci\runner, writes E:\ci\runner.env and registers two Task Scheduler tasks
# (at logon, this user, no window): "mdoc local CI" runs
# `scripts/local-ci.sh run` in Git Bash, and "mdoc local CI web" serves
# E:\ci\out on 127.0.0.1 with Python's http.server, which `tailscale serve`
# proxies to the tailnet at /ci (serving a folder directly needs a Windows
# admin). Re-run it from an updated checkout to update the runner, or with a
# new -Root to move it (copy out\ first to keep old status links working).
# -Uninstall stops and removes the tasks and the tailnet path and keeps the
# runner home.
[CmdletBinding()]
param(
    [string] $Root = 'E:\ci',
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
# The web server's --directory: forward slashes, as Python and Git Bash take it.
$outArg = $out.Replace('\', '/')

# A string as one Bash word: it's -> 'it'\''s'.
function ConvertTo-BashWord([string] $Text) {
    "'" + $Text.Replace("'", "'\''") + "'"
}

# Runs a native command whose failure is expected (a process already gone, a
# path not served) and drops its output: under Windows PowerShell its stderr
# would otherwise end this script ($ErrorActionPreference = 'Stop').
function Invoke-Quietly([string] $Exe, [string[]] $Arguments) {
    $ErrorActionPreference = 'Continue'
    & $Exe @Arguments 2>&1 | Out-Null
}

function Stop-Runner {
    foreach ($name in $taskName, $webTaskName) {
        if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
            Stop-ScheduledTask -TaskName $name
        }
    }
    # Stopping a task ends its bash but not the native processes under it (a
    # job's cargo, rustc and test binaries; the web server's python): end each
    # runner bash's tree and this runner's web server. The registered web task
    # may serve an older -Root's out folder; end that server too, since
    # http.server sets SO_REUSEADDR and on Windows a leftover one keeps
    # answering on the same port beside the new one.
    $dirs = @($outArg)
    $webTask = Get-ScheduledTask -TaskName $webTaskName -ErrorAction SilentlyContinue
    if ($webTask -and $webTask.Actions[0].Arguments -match "--directory '([^']+)'") { $dirs += $Matches[1] }
    $web = @($dirs | Select-Object -Unique | ForEach-Object {
        "*http.server --bind 127.0.0.1 $Port --directory $([WildcardPattern]::Escape($_))"
    })
    Get-CimInstance Win32_Process -Filter "Name = 'bash.exe' OR Name = 'python.exe'" |
        Where-Object {
            $line = $_.CommandLine
            $line -like '*local-ci.sh*run*' -or @($web | Where-Object { $line -like $_ }).Count -gt 0
        } |
        ForEach-Object { Invoke-Quietly taskkill.exe '/F', '/T', '/PID', $_.ProcessId }
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
    Invoke-Quietly tailscale 'serve', '--https=443', '--set-path=/ci', 'off'
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
# The machine's own settings after these three lines survive a re-run.
$envPath = Join-Path $Root 'runner.env'
$kept = @()
if (Test-Path $envPath) {
    $kept = @(Get-Content $envPath | Where-Object {
            $_ -notmatch '^(# Written by scripts/local-ci|LOCAL_CI_URL=|LOCAL_CI_PORT=)'
        })
}
$envFile = @(
    '# Written by scripts/local-ci-setup.ps1, which keeps the lines after these three.'
    "LOCAL_CI_URL=https://$dns/ci"
    "LOCAL_CI_PORT=$Port"
) + $kept
# LF line endings: bash sources this file.
[IO.File]::WriteAllText($envPath, ($envFile -join "`n") + "`n")

# A Git Bash command as a task at logon, this user, restarted if it ends.
# conhost --headless keeps it from opening a console window, but writes to
# that console fail (which ends a `set -e` script), so each command sends its
# output to a file.
# Priority 4 is normal: Task Scheduler's default, 7, runs the task and every
# process it starts (cargo, rustc, the tests) at below-normal CPU and low I/O
# priority, so the lanes' builds starve a job.
function Register-CiTask([string] $Name, [string] $Command, [string] $Description) {
    $action = New-ScheduledTaskAction -Execute (Join-Path $env:WINDIR 'System32\conhost.exe') `
        -Argument "--headless `"$bash`" -c `"$Command`""
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
        -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -StartWhenAvailable -Priority 4
    $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName $Name -Action $action -Trigger $trigger -Settings $settings `
        -Principal $principal -Description $Description -Force | Out-Null
    Start-ScheduledTask -TaskName $Name
}

# The pinned tools in ~/.cargo/bin come first, as on the Mac.
$runner = ConvertTo-BashWord "$bashRoot/runner/scripts/local-ci.sh"
$taskLog = ConvertTo-BashWord "$bashRoot/state/task.log"
Register-CiTask $taskName `
    "LOCAL_CI_HOME=$(ConvertTo-BashWord $bashRoot) PATH=`$HOME/.cargo/bin:`$PATH exec bash $runner run >>$taskLog 2>&1" `
    'mdoc local CI runner (ADR 0082): scripts/local-ci.sh run'
# Localhost only; tailscale serve is the way in from the tailnet.
Register-CiTask $webTaskName `
    "exec $(ConvertTo-BashWord (ConvertTo-BashPath $python.Source)) -m http.server --bind 127.0.0.1 $Port --directory $(ConvertTo-BashWord $outArg) >/dev/null 2>&1" `
    "mdoc local CI results (ADR 0082): $out on 127.0.0.1:$Port"

& tailscale serve --bg --set-path /ci "http://127.0.0.1:$Port" | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'tailscale serve failed' }

Write-Output "Local CI installed: local/windows, checkout $checkout, results https://$dns/ci/"
Write-Output "Check it in Git Bash with: LOCAL_CI_HOME=$bashRoot just local-ci status"

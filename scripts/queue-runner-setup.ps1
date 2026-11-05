# Usage: powershell -ExecutionPolicy Bypass -File scripts\queue-runner-setup.ps1 [-Jobs windows,android] [-Uninstall] [-Root E:\ci]
#
# The merge-queue runners on the PC (ADR 0095). For each job (windows,
# android) makes its home E:\ci\queue-<job> with its own CI checkout (a clone
# at E:\ci\queue-<job>\mdoc, on the Dev Drive), copies the runner from this
# repository to E:\ci\queue-<job>\runner\scripts, writes
# E:\ci\queue-<job>\runner.env and registers a Task Scheduler task (at logon,
# this user, no window), "mdoc queue CI <job>", that runs
# `scripts/queue-runner.sh run` in Git Bash with QUEUE_CI_JOB=<job>. Results
# go to E:\ci\out\queue-<job>, which the local CI web task ("mdoc local CI
# web", scripts/local-ci-setup.ps1) already serves at /ci on the tailnet;
# without that task this script registers its own ("mdoc queue CI web",
# the same server, which also sets /ci again each time it starts; re-run
# this after `local-ci-setup.ps1 -Uninstall`, which removes the local one).
# The installed runner updates itself from main when idle;
# re-run this from an updated checkout to update it at once.
# -Uninstall stops and removes the tasks (and this script's web task and
# tailnet path) and keeps the homes. The CI checkouts start cold: the first
# Windows build fills a target/ of about 30 GB in each.
[CmdletBinding()]
param(
    [string] $Root = 'E:\ci',
    [string] $Repo = 'lawkitt/mdoc',
    [int] $Port = 8787,
    [ValidateSet('windows', 'android')]
    [string[]] $Jobs = @('windows', 'android'),
    [switch] $Uninstall
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$localWebTaskName = 'mdoc local CI web'
$webTaskName = 'mdoc queue CI web'
$bash = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
$source = Split-Path -Parent $PSScriptRoot
$out = Join-Path $Root 'out'

function Get-TaskName([string] $Job) { "mdoc queue CI $Job" }

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

# Stopping a task ends its bash but not the native processes under it (a
# job's cargo, rustc, test binaries, Gradle and the emulator): end the
# runner bash's whole tree, found by its installed copy's path.
function Stop-Runner([string] $Job) {
    $name = Get-TaskName $Job
    if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $name
    }
    $copy = "*queue-$Job/runner/scripts/queue-runner.sh*"
    Get-CimInstance Win32_Process -Filter "Name = 'bash.exe'" |
        Where-Object { $_.CommandLine -like $copy } |
        ForEach-Object { Invoke-Quietly taskkill.exe '/F', '/T', '/PID', $_.ProcessId }
    $lock = Join-Path $Root "queue-$Job\state\lock"
    if (Test-Path $lock) { Remove-Item -Recurse -Force $lock }
}

function Stop-Web {
    if (Get-ScheduledTask -TaskName $webTaskName -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $webTaskName
    }
    # http.server sets SO_REUSEADDR: on Windows a leftover one keeps answering
    # on the same port beside a new one.
    $web = "*http.server --bind 127.0.0.1 $Port --directory $([WildcardPattern]::Escape($outArg))"
    Get-CimInstance Win32_Process -Filter "Name = 'python.exe'" |
        Where-Object { $_.CommandLine -like $web } |
        ForEach-Object { Invoke-Quietly taskkill.exe '/F', '/T', '/PID', $_.ProcessId }
}

if ($Uninstall) {
    foreach ($job in $Jobs) {
        Stop-Runner $job
        $name = Get-TaskName $job
        if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
            Unregister-ScheduledTask -TaskName $name -Confirm:$false
        }
    }
    $left = @('windows', 'android' | Where-Object {
            Get-ScheduledTask -TaskName (Get-TaskName $_) -ErrorAction SilentlyContinue
        })
    if ($left.Count -eq 0 -and (Get-ScheduledTask -TaskName $webTaskName -ErrorAction SilentlyContinue)) {
        Stop-Web
        Unregister-ScheduledTask -TaskName $webTaskName -Confirm:$false
        Invoke-Quietly tailscale 'serve', '--https=443', '--set-path=/ci', 'off'
    }
    Write-Output "Removed the queue CI tasks for $($Jobs -join ', '); the homes under $Root are kept."
    return
}

# The local CI web task serves its own -Root's out folder: only one under
# this -Root serves this output, and a second server cannot share its port.
$localWeb = Get-ScheduledTask -TaskName $localWebTaskName -ErrorAction SilentlyContinue
if ($localWeb) {
    $served = $null
    if ($localWeb.Actions[0].Arguments -match "--directory '([^']+)'") { $served = $Matches[1] }
    if ($served -ne $outArg) {
        throw "'$localWebTaskName' serves $served, not ${outArg}: install with that task's -Root, or remove it first (local-ci-setup.ps1 -Uninstall)"
    }
}

foreach ($tool in 'git', 'gh', 'just', 'cargo', 'tailscale') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool not found on PATH" }
}
if (-not (Test-Path $bash)) { throw "Git Bash not found at $bash" }
& gh auth status *> $null
if ($LASTEXITCODE -ne 0) { throw 'gh is not logged in' }
$dns = ((& tailscale status --self --peers=false --json) | ConvertFrom-Json).Self.DNSName.TrimEnd('.')
if (-not $dns) { throw "could not read this machine's tailnet name" }

# A Git Bash command as a task at logon, this user, restarted if it ends.
# conhost --headless keeps it from opening a console window, but writes to
# that console fail (which ends a `set -e` script), so each command sends its
# output to a file. Priority 4 is normal: Task Scheduler's default, 7, runs
# every process it starts at below-normal CPU and low I/O priority.
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

foreach ($job in $Jobs) {
    $jobHome = Join-Path $Root "queue-$job"
    $checkout = Join-Path $jobHome 'mdoc'
    $state = Join-Path $jobHome 'state'
    $runnerScripts = Join-Path $jobHome 'runner\scripts'
    New-Item -ItemType Directory -Force -Path $jobHome, $state, $runnerScripts, (Join-Path $out "queue-$job") | Out-Null
    if (-not (Test-Path (Join-Path $checkout '.git'))) {
        Write-Output "Cloning the $job CI checkout into $checkout"
        & git clone --quiet "https://github.com/$Repo.git" $checkout
        if ($LASTEXITCODE -ne 0) { throw 'git clone failed' }
    }

    # Stop first: the running runner reads its copy of queue-runner.sh.
    Stop-Runner $job
    foreach ($file in 'queue-runner.sh', 'lib.sh') {
        Copy-Item -Force (Join-Path $source "scripts\$file") $runnerScripts
    }
    # The machine's own settings after these two lines survive a re-run
    # (QUEUE_CI_MIN_FREE_GB=30, say).
    $envPath = Join-Path $jobHome 'runner.env'
    $kept = @()
    if (Test-Path $envPath) {
        $kept = @(Get-Content $envPath | Where-Object {
                $_ -notmatch '^(# Written by scripts/queue-runner-setup|QUEUE_CI_URL=)'
            })
    }
    $envFile = @(
        '# Written by scripts/queue-runner-setup.ps1, which keeps the lines after these two.'
        "QUEUE_CI_URL=https://$dns/ci/queue-$job"
    ) + $kept
    # LF line endings: bash sources this file.
    [IO.File]::WriteAllText($envPath, ($envFile -join "`n") + "`n")

    # The pinned tools in ~/.cargo/bin come first, as for local CI.
    $runner = ConvertTo-BashWord "$bashRoot/queue-$job/runner/scripts/queue-runner.sh"
    $taskLog = ConvertTo-BashWord "$bashRoot/queue-$job/state/task.log"
    $vars = "LOCAL_CI_HOME=$(ConvertTo-BashWord $bashRoot) QUEUE_CI_HOME=$(ConvertTo-BashWord "$bashRoot/queue-$job") QUEUE_CI_JOB=$job"
    Register-CiTask (Get-TaskName $job) `
        "$vars PATH=`$HOME/.cargo/bin:`$PATH exec bash $runner run >>$taskLog 2>&1" `
        "mdoc merge-queue runner (ADR 0095): scripts/queue-runner.sh run, ci/$job"
}

# The results: the local CI web task serves $out already; else this one.
if (-not $localWeb) {
    # A real Python, not the Microsoft Store stub in WindowsApps.
    $python = Get-Command python.exe -All -ErrorAction SilentlyContinue |
        Where-Object { $_.Source -notlike '*\WindowsApps\*' } | Select-Object -First 1
    if (-not $python) { throw 'python not found on PATH (the results server needs it)' }
    Stop-Web
    & tailscale serve --bg --set-path /ci "http://127.0.0.1:$Port" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'tailscale serve failed' }
    # Localhost only; tailscale serve is the way in from the tailnet. The task
    # sets /ci again each time it starts (at logon, after a restart): /ci went
    # once and the log page stayed down unnoticed (#614). Setting it again is
    # harmless and changes no other path (another app serves on 443). It
    # tries for 5 minutes, as tailscaled may not be up yet at logon, then
    # starts the server anyway. MSYS_NO_PATHCONV: Git Bash would turn /ci into
    # a Windows path for tailscale.exe.
    $tailscale = ConvertTo-BashWord (ConvertTo-BashPath (Get-Command tailscale).Source)
    $serve = "MSYS_NO_PATHCONV=1 $tailscale serve --bg --set-path /ci http://127.0.0.1:$Port >/dev/null 2>&1"
    Register-CiTask $webTaskName `
        "for _ in `$(seq 30); do $serve && break; sleep 10; done; exec $(ConvertTo-BashWord (ConvertTo-BashPath $python.Source)) -m http.server --bind 127.0.0.1 $Port --directory $(ConvertTo-BashWord $outArg) >/dev/null 2>&1" `
        "mdoc merge-queue results (ADR 0095): $out on 127.0.0.1:$Port, and /ci on the tailnet"
}

Write-Output "Queue CI installed for $($Jobs -join ', '): results https://$dns/ci/queue-<job>/"
Write-Output "Check one in Git Bash with: QUEUE_CI_JOB=windows LOCAL_CI_HOME=$bashRoot bash scripts/queue-runner.sh status"

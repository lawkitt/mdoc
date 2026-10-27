# CI only: never run an installer or capture the maintainer's desktop.
[CmdletBinding()]
param(
    [string] $ExpectedWindowTitle = 'mdoc',
    [ValidateRange(1, 120)]
    [int] $StartupTimeoutSeconds = 60
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'Installed desktop smoke requires a GitHub-hosted Windows runner.'
}
$profilePath = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'mdoc'
$installDir = Join-Path $env:RUNNER_TEMP 'mdoc-installed-smoke'
$evidence = New-Item -ItemType Directory -Path installed-windows-evidence -Force
Start-Transcript -Path (Join-Path $evidence.FullName 'smoke.log')

function Save-Screen([string] $name) {
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
        $bitmap.Save((Join-Path $evidence.FullName "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Run-Installer([string] $exe, [string] $arguments) {
    $process = Start-Process -FilePath $exe -ArgumentList $arguments -PassThru
    if (-not $process.WaitForExit(120000)) {
        $process.Kill($true)
        throw "Installer timed out: $exe"
    }
    if ($process.ExitCode -ne 0) { throw "Installer failed ($($process.ExitCode)): $exe" }
}

function Save-UiaEvidence {
    try {
        $lines = [System.Collections.Generic.List[string]]::new()
        if ($app) {
            $condition = [System.Windows.Automation.PropertyCondition]::new(
                [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $app.Id)
            $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
                [System.Windows.Automation.TreeScope]::Children, $condition)
            foreach ($item in $windows) {
                $state = $item.Current
                $lines.Add("Window: Name=$($state.Name) PID=$($state.ProcessId) Role=$($state.ControlType.ProgrammaticName) Offscreen=$($state.IsOffscreen) Bounds=$($state.BoundingRectangle)")
                foreach ($control in $item.FindAll([System.Windows.Automation.TreeScope]::Descendants,
                    [System.Windows.Automation.Condition]::TrueCondition)) {
                    $lines.Add("  Role=$($control.Current.ControlType.ProgrammaticName) Name=$($control.Current.Name)")
                }
            }
        }
        if ($lines.Count -eq 0) { $lines.Add('No UI Automation windows found for the launched process (or no process was launched).') }
        $lines | Out-File (Join-Path $evidence.FullName 'uia.txt')
    } catch {
        "UI Automation evidence unavailable: $_" | Out-File (Join-Path $evidence.FullName 'uia.txt')
    }
}

$app = $null
$passed = $false
$screenReady = $false
$installAttempted = $false
try {
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing, System.Windows.Forms
    if (-not [Environment]::UserInteractive) { throw 'No interactive desktop session.' }
    $screenReady = $true
    if (Test-Path -LiteralPath $profilePath) { throw "Fresh runner profile required: $profilePath already exists." }
    if (Get-Process mdoc -ErrorAction SilentlyContinue) { throw 'mdoc is already running.' }
    if (Test-Path -LiteralPath $installDir) { throw "Install directory already exists: $installDir" }
    $installers = @(Get-ChildItem dist/mdoc_*-setup.exe)
    if ($installers.Count -ne 1) { throw 'Expected exactly one NSIS installer in dist.' }
    # /D must be last and unquoted (NSIS); use a separate installation directory.
    $installAttempted = $true
    Run-Installer $installers[0].FullName "/S /D=$installDir"
    $exe = Join-Path $installDir 'mdoc.exe'
    $uninstaller = Join-Path $installDir 'uninstall.exe'
    if (-not (Test-Path -LiteralPath $exe) -or -not (Test-Path -LiteralPath $uninstaller)) {
        throw 'NSIS did not install mdoc.exe and uninstall.exe.'
    }
    Write-Host "Installed executable: $exe"
    Get-FileHash -Algorithm SHA256 -LiteralPath $exe | Format-List
    $app = Start-Process -FilePath $exe -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    $window = $null
    do {
        if ($app.HasExited) { throw "Installed mdoc exited before showing its main window ($($app.ExitCode))." }
        $condition = [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $app.Id)
        $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Children, $condition)
        foreach ($candidate in $windows) {
            $state = $candidate.Current
            if ($state.ControlType -eq [System.Windows.Automation.ControlType]::Window -and
                $state.Name -eq $ExpectedWindowTitle -and -not $state.IsOffscreen -and
                $state.BoundingRectangle.Width -gt 100 -and $state.BoundingRectangle.Height -gt 100) {
                $window = $candidate
                break
            }
        }
        if (-not $window) { Start-Sleep -Milliseconds 500 }
    } while (-not $window -and [DateTime]::UtcNow -lt $deadline)
    if (-not $window) { throw "Installed mdoc did not expose a visible UI Automation main window titled $ExpectedWindowTitle within $StartupTimeoutSeconds seconds." }
    # Give the first frame time to present, then record both UIA and pixels.
    Start-Sleep -Seconds 2
    Save-UiaEvidence
    Save-Screen 'main-window'
    if (-not $app.CloseMainWindow()) { throw 'Could not request a clean main-window close.' }
    if (-not $app.WaitForExit(15000)) { throw 'Installed mdoc did not quit cleanly within 15 seconds.' }
    if ($app.ExitCode -ne 0) { throw "Installed mdoc quit with exit code $($app.ExitCode)." }
    $passed = $true
} catch {
    $_ | Format-List * -Force | Out-File (Join-Path $evidence.FullName 'failure.txt')
    Write-Host "Smoke failed: $_"
    throw
} finally {
    try {
        if (-not $passed) { Save-UiaEvidence }
        if (-not $passed -and $screenReady) { Save-Screen 'failure' }
    } catch {
        "Screen capture failed: $_" | Out-File (Join-Path $evidence.FullName 'capture-error.txt')
    } finally {
        try {
            # Forced termination is cleanup only; it can never make the smoke pass.
            if ($app -and -not $app.HasExited) { $app.Kill($true); $app.WaitForExit() }
            $uninstaller = Join-Path $installDir 'uninstall.exe'
            if ($installAttempted -and (Test-Path -LiteralPath $uninstaller)) {
                # NSIS must preserve the DPAPI case key across uninstall/reinstall.
                # Seed only after the app exits: dummy bytes are not a real key.
                $caseKey = Join-Path $profilePath 'cases/key.dpapi'
                $dummyCaseKeyCreated = $false
                try {
                    if (-not (Test-Path -LiteralPath $caseKey)) {
                        New-Item -ItemType Directory -Path (Split-Path $caseKey) -Force | Out-Null
                        New-Item -ItemType File -Path $caseKey -Value 'mdoc CI uninstall sentinel (not a DPAPI key)' | Out-Null
                        $dummyCaseKeyCreated = $true
                    }
                    # _?= runs in place, so WaitForExit observes the real uninstaller.
                    Run-Installer $uninstaller "/S _?=$installDir"
                    if (Test-Path -LiteralPath (Join-Path $installDir 'mdoc.exe')) {
                        throw 'Uninstall left mdoc.exe installed.'
                    }
                    if (-not (Test-Path -LiteralPath $caseKey -PathType Leaf)) {
                        throw 'Uninstall deleted the case key (cases/key.dpapi).'
                    }
                    'Uninstall preserved cases/key.dpapi.' | Tee-Object -FilePath (Join-Path $evidence.FullName 'case-key.txt')
                    Write-Host 'Uninstall completed.'
                } finally {
                    # Never remove a key that was already present before seeding.
                    if ($dummyCaseKeyCreated -and (Test-Path -LiteralPath $caseKey)) {
                        Remove-Item -LiteralPath $caseKey
                    }
                }
            }
        } catch {
            $_ | Format-List * -Force | Out-File (Join-Path $evidence.FullName 'cleanup-error.txt')
            Write-Host "Cleanup failed: $_"
            throw
        } finally { Stop-Transcript }
    }
}

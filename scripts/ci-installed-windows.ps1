# CI only: never run an installer or capture the maintainer's desktop.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'Installed desktop smoke requires a GitHub-hosted Windows runner.'
}
if (-not [Environment]::UserInteractive) { throw 'No interactive desktop session.' }
$profilePath = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'mdoc'
if (Test-Path -LiteralPath $profilePath) { throw "Fresh runner profile required: $profilePath already exists." }
if (Get-Process mdoc -ErrorAction SilentlyContinue) { throw 'mdoc is already running.' }
$installDir = Join-Path $env:RUNNER_TEMP 'mdoc-installed-smoke'
if (Test-Path -LiteralPath $installDir) { throw "Install directory already exists: $installDir" }
$installers = @(Get-ChildItem dist/mdoc_*-setup.exe)
if ($installers.Count -ne 1) { throw 'Expected exactly one NSIS installer in dist.' }
$evidence = New-Item -ItemType Directory -Path installed-windows-evidence -Force
Start-Transcript -Path (Join-Path $evidence.FullName 'smoke.log')
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing, System.Windows.Forms

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

$app = $null
$passed = $false
try {
    # /D must be last and unquoted (NSIS); use a separate installation directory.
    Run-Installer $installers[0].FullName "/S /D=$installDir"
    $exe = Join-Path $installDir 'mdoc.exe'
    $uninstaller = Join-Path $installDir 'uninstall.exe'
    if (-not (Test-Path -LiteralPath $exe) -or -not (Test-Path -LiteralPath $uninstaller)) {
        throw 'NSIS did not install mdoc.exe and uninstall.exe.'
    }
    Write-Host "Installed executable: $exe"
    Get-FileHash -Algorithm SHA256 -LiteralPath $exe | Format-List
    $app = Start-Process -FilePath $exe -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
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
                $state.Name -eq 'mdoc' -and -not $state.IsOffscreen -and
                $state.BoundingRectangle.Width -gt 100 -and $state.BoundingRectangle.Height -gt 100) {
                $window = $candidate
                break
            }
        }
        if (-not $window) { Start-Sleep -Milliseconds 500 }
    } while (-not $window -and [DateTime]::UtcNow -lt $deadline)
    if (-not $window) { throw 'Installed mdoc did not expose a visible UI Automation main window titled mdoc within 60 seconds.' }
    # Give the first frame time to present, then record both UIA and pixels.
    Start-Sleep -Seconds 2
    $window.Current | Format-List Name, ProcessId, ControlType, IsOffscreen, BoundingRectangle |
        Out-File (Join-Path $evidence.FullName 'uia-window.txt')
    $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
            "Role=$($_.Current.ControlType.ProgrammaticName) Name=$($_.Current.Name)"
        } | Out-File (Join-Path $evidence.FullName 'uia-controls.txt')
    Save-Screen 'main-window'
    if (-not $app.CloseMainWindow()) { throw 'Could not request a clean main-window close.' }
    if (-not $app.WaitForExit(15000)) { throw 'Installed mdoc did not quit cleanly within 15 seconds.' }
    if ($app.ExitCode -ne 0) { throw "Installed mdoc quit with exit code $($app.ExitCode)." }
    $passed = $true
} finally {
    try {
        if (-not $passed) { Save-Screen 'failure' }
    } finally {
        try {
            # Forced termination is cleanup only; it can never make the smoke pass.
            if ($app -and -not $app.HasExited) { $app.Kill($true); $app.WaitForExit() }
            $uninstaller = Join-Path $installDir 'uninstall.exe'
            if (Test-Path -LiteralPath $uninstaller) {
                # _?= runs in place, so WaitForExit observes the real uninstaller.
                Run-Installer $uninstaller "/S _?=$installDir"
                if (Test-Path -LiteralPath (Join-Path $installDir 'mdoc.exe')) {
                    throw 'Uninstall left mdoc.exe installed.'
                }
                Write-Host 'Uninstall completed.'
            }
        } finally { Stop-Transcript }
    }
}

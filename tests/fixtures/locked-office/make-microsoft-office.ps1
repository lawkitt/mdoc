# Writes the Microsoft Office fixtures in this folder through Word, Excel and
# PowerPoint themselves (COM automation, no window). Run on Windows with
# Microsoft 365 installed:
#   pwsh -NoProfile -File tests/fixtures/locked-office/make-microsoft-office.ps1
# The text is fictional. The password is a published test value (README.md).
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$password = 'Petrov-2026'

$lines = @(
    'Engagement letter',
    'Client: Ivan Petrov, 12 Baker Street, Springfield.',
    'Matter: lease dispute with Northwind Holdings Ltd.',
    'Клиент: Иван Петров, ул. Садовая, 12.'
)

function Remove-Fixture([string]$name) {
    $path = Join-Path $here $name
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    $path
}

# Word: DOCX (agile encryption) and DOC (RC4 CryptoAPI), each locked and open.
$word = New-Object -ComObject Word.Application
$word.Visible = $false
$word.DisplayAlerts = 0
try {
    foreach ($case in @(
            @{ Name = 'word-locked.docx'; Format = 16; Locked = $true },
            @{ Name = 'word-open.docx'; Format = 16; Locked = $false },
            @{ Name = 'word-locked.doc'; Format = 0; Locked = $true },
            @{ Name = 'word-open.doc'; Format = 0; Locked = $false })) {
        $path = Remove-Fixture $case.Name
        $doc = $word.Documents.Add()
        $doc.Content.Text = ($lines -join "`r")
        if ($case.Locked) { $doc.Password = $password }
        $doc.SaveAs2($path, $case.Format)
        $doc.Close(0)
    }
}
finally { $word.Quit(0) }

# Excel: XLSX, locked and open.
$excel = New-Object -ComObject Excel.Application
$excel.Visible = $false
$excel.DisplayAlerts = $false
try {
    foreach ($case in @(
            @{ Name = 'excel-locked.xlsx'; Locked = $true },
            @{ Name = 'excel-open.xlsx'; Locked = $false })) {
        $path = Remove-Fixture $case.Name
        $book = $excel.Workbooks.Add()
        $sheet = $book.Worksheets.Item(1)
        $rows = @(
            @('Client', 'Ivan Petrov'),
            @('Address', '12 Baker Street, Springfield'),
            @('Matter', 'Lease dispute with Northwind Holdings Ltd.'),
            @('Fee', 1200))
        for ($r = 0; $r -lt $rows.Count; $r++) {
            # Formula takes text; Excel stores "1200" as a number.
            $sheet.Cells.Item($r + 1, 1).Formula = [string]$rows[$r][0]
            $sheet.Cells.Item($r + 1, 2).Formula = [string]$rows[$r][1]
        }
        if ($case.Locked) { $book.Password = $password }
        $book.SaveAs($path, 51)
        $book.Close($false)
    }
}
finally { $excel.Quit() }

# PowerPoint: PPTX, locked and open.
$ppt = New-Object -ComObject PowerPoint.Application
try {
    foreach ($case in @(
            @{ Name = 'powerpoint-locked.pptx'; Locked = $true },
            @{ Name = 'powerpoint-open.pptx'; Locked = $false })) {
        $path = Remove-Fixture $case.Name
        $deck = $ppt.Presentations.Add(0)
        $slide = $deck.Slides.Add(1, 2)
        $slide.Shapes.Item(1).TextFrame.TextRange.Text = $lines[0]
        $slide.Shapes.Item(2).TextFrame.TextRange.Text = ($lines[1..3] -join "`r")
        if ($case.Locked) { $deck.Password = $password }
        $deck.SaveAs($path, 24)
        $deck.Close()
    }
}
finally { $ppt.Quit() }

Get-ChildItem $here -File | Where-Object Name -match '^(word|excel|powerpoint)-' |
    Format-Table Name, Length

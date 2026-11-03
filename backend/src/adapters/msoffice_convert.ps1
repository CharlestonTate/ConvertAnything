param(
    [Parameter(Mandatory = $true)][string]$InputPath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [Parameter(Mandatory = $true)][ValidateSet("word", "excel", "ppt")][string]$App,
    [Parameter(Mandatory = $true)][int]$Format,
    [switch]$ExportPdf
)

$ErrorActionPreference = "Stop"
$InputPath = [System.IO.Path]::GetFullPath($InputPath)
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$outDir = Split-Path -Parent $OutputPath
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir | Out-Null
}
if (Test-Path $OutputPath) {
    Remove-Item -Force $OutputPath
}

function Release-Com($obj) {
    if ($null -ne $obj) {
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($obj)
    }
}

switch ($App) {
    "word" {
        $word = New-Object -ComObject Word.Application
        $word.Visible = $false
        $word.DisplayAlerts = 0
        $doc = $null
        try {
            $doc = $word.Documents.Open($InputPath, $false, $true)
            if ($ExportPdf) {
                $doc.ExportAsFixedFormat($OutputPath, 17)
            } else {
                $doc.SaveAs2($OutputPath, $Format)
            }
        } finally {
            if ($null -ne $doc) { $doc.Close($false) }
            $word.Quit()
            Release-Com $doc
            Release-Com $word
        }
    }
    "excel" {
        $excel = New-Object -ComObject Excel.Application
        $excel.Visible = $false
        $excel.DisplayAlerts = $false
        $wb = $null
        try {
            $wb = $excel.Workbooks.Open($InputPath)
            if ($ExportPdf) {
                $wb.ExportAsFixedFormat(0, $OutputPath)
            } else {
                $wb.SaveAs($OutputPath, $Format)
            }
        } finally {
            if ($null -ne $wb) { $wb.Close($false) }
            $excel.Quit()
            Release-Com $wb
            Release-Com $excel
        }
    }
    "ppt" {
        $ppt = New-Object -ComObject PowerPoint.Application
        $pres = $null
        try {
            $pres = $ppt.Presentations.Open($InputPath, $true, $false, $false)
            if ($ExportPdf) {
                $pres.SaveAs($OutputPath, 32)
            } else {
                $pres.SaveAs($OutputPath, $Format)
            }
        } finally {
            if ($null -ne $pres) { $pres.Close() }
            $ppt.Quit()
            Release-Com $pres
            Release-Com $ppt
        }
    }
}

if (-not (Test-Path $OutputPath)) {
    throw "Microsoft Office did not write $OutputPath"
}

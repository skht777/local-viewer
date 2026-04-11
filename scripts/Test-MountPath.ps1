#Requires -Version 7.0
# Convert-MountPath.ps1 の単体テストランナー
#
# 正常系: scripts/mount-path-cases.tsv の 2 列目 (ps_expected) を検証
# 不正系: scripts/mount-path-invalid-cases.tsv + ハードコード制御文字ケースで
#         Test-HostPath が $false を返すことを検証
#
# 終了コード: 0=全 pass, 1=1 件以上失敗
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = $PSScriptRoot
. (Join-Path $scriptDir 'Convert-MountPath.ps1')

$casesTsv = Join-Path $scriptDir 'mount-path-cases.tsv'
$invalidTsv = Join-Path $scriptDir 'mount-path-invalid-cases.tsv'

$pass = 0
$fail = 0

function Read-Tsv {
    param([string]$Path, [int]$ColumnCount)
    Get-Content -LiteralPath $Path | Where-Object { $_.Trim().Length -gt 0 } | ForEach-Object {
        $cols = $_ -split "`t"
        if ($cols.Count -lt $ColumnCount) { return }
        [PSCustomObject]@{ Columns = $cols }
    }
}

# 正常系: Convert-HostPathToDockerFormat
foreach ($row in Read-Tsv $casesTsv 3) {
    $input = $row.Columns[0]
    $psExpected = $row.Columns[1]
    $actual = Convert-HostPathToDockerFormat $input
    if ($actual -eq $psExpected) {
        $pass++
    } else {
        Write-Host "FAIL [convert]: $input"
        Write-Host "  expected: $psExpected"
        Write-Host "  actual:   $actual"
        $fail++
    }
}

# 正常系パスは Test-HostPath も通ること (converted 後)
foreach ($row in Read-Tsv $casesTsv 3) {
    $input = $row.Columns[0]
    $psExpected = $row.Columns[1]
    if (Test-HostPath $psExpected) {
        $pass++
    } else {
        Write-Host "FAIL [validate-valid]: $input (converted=$psExpected) が拒否された"
        $fail++
    }
}

# 不正系 TSV
foreach ($row in Read-Tsv $invalidTsv 2) {
    $input = $row.Columns[0]
    $reason = $row.Columns[1]
    if (Test-HostPath $input) {
        Write-Host "FAIL [validate-invalid]: '$input' ($reason) が拒否されなかった"
        $fail++
    } else {
        $pass++
    }
}

# 不正系 (制御文字 - TSV では表現しにくいためハードコード)
$controlCases = @(
    "C:\foo`nevil: bar",
    "C:\foo`tBAD",
    "C:\foo`rCR"
)
foreach ($input in $controlCases) {
    if (Test-HostPath $input) {
        Write-Host 'FAIL [validate-ctrl]: 制御文字入力が拒否されなかった'
        $fail++
    } else {
        $pass++
    }
}

# slug 検証
$validSlugs = @('ai', 'photos', 'my-data', 'set.01', 'A_B_C')
$invalidSlugs = @('', '.', '..', 'foo/bar', 'foo\bar', 'foo bar', 'foo#bar')

foreach ($slug in $validSlugs) {
    if (Test-Slug $slug) {
        $pass++
    } else {
        Write-Host "FAIL [slug-valid]: '$slug' が拒否された"
        $fail++
    }
}

foreach ($slug in $invalidSlugs) {
    if (Test-Slug $slug) {
        Write-Host "FAIL [slug-invalid]: '$slug' が拒否されなかった"
        $fail++
    } else {
        $pass++
    }
}

Write-Host ''
Write-Host "結果: $pass passed, $fail failed"
exit ($(if ($fail -gt 0) { 1 } else { 0 }))

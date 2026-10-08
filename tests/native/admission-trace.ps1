param([Parameter(Mandatory)][string]$Trace)
$shows = Get-Content -LiteralPath $Trace | Where-Object { $_ -match '^8002 ' }
if (!$shows) { throw 'No real SHOW event captured' }
foreach ($line in $shows) {
    if ($line -match 'ex=([0-9A-Fa-f]+)') {
        $ex = [Convert]::ToUInt32($Matches[1],16)
        if (($ex -band 0x80) -ne 0 -or ($ex -band 0x40000) -eq 0) {
            throw "First SHOW occurred before ordinary-window eligibility: $line"
        }
    }
}
Write-Host 'PASS real SHOW uses final ordinary-window eligibility'

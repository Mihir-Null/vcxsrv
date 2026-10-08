#Requires -Version 7
param([Parameter(Mandatory)][string]$Mhmake,[Parameter(Mandatory)][string]$ToolPathsFile)
$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Import-Module (Join-Path $root 'packaging/BuildServer.psm1') -Force
$work=Join-Path $root ('.superpowers/sdd/2026-10-07-vcxsrv-wsl-manager/keyboard-target-tests-'+[guid]::NewGuid())
$saved=@{};Get-ChildItem Env: | ForEach-Object {$saved[$_.Name]=$_.Value}
$env:PATH='C:/msys64/usr/bin;'+$env:PATH
$inventory=Get-BuildInventory $root (Get-Content $ToolPathsFile -Raw | ConvertFrom-Json -AsHashtable)
$tools=@{};foreach($tool in $inventory.tools){if($tool.path){$tools[$tool.name]=$tool.path}}
$count=0;$failures=0
function Assert($condition,$message){if(-not $condition){throw $message}}
try {
    foreach($case in @('valid','valid-legacy-snapshot','missing','missing-rules','empty')){
        $count++
        try {
            $snapshot=Join-Path $work $case
            foreach($relative in @('makefile.before','makefile.after','tools/mhmake/makebison.bat','tools/mhmake/makelex.bat','xorg-server/hw/xwin/doflexbison.bat','xkbcomp/bison.bat','mesalib/src/compiler/makefile.srcs','mesalib/src/mesa/makefile.srcs','xorg-server/dix/makefile','xorg-server/dix/generate-atoms','xorg-server/xkeyboard-config/makefile')){
                $destination=Join-Path $snapshot $relative
                New-Item -ItemType Directory (Split-Path $destination -Parent) -Force | Out-Null
                Copy-Item -LiteralPath (Join-Path $root $relative) -Destination $destination
            }
            $data=Join-Path $snapshot 'xorg-server/xkbdata'
            New-Item -ItemType Directory (Join-Path $data 'rules'),(Join-Path $data 'symbols') -Force | Out-Null
            if($case -ne 'missing-rules'){[IO.File]::WriteAllText((Join-Path $data 'rules/base'),'rules-fixture')}
            if($case.StartsWith('valid') -or $case -eq 'missing-rules'){[IO.File]::WriteAllText((Join-Path $data 'symbols/us'),'symbols-fixture')}
            if($case -eq 'empty'){[IO.File]::WriteAllText((Join-Path $data 'symbols/us'),'')}
            # Preserve the real directory-target block; omit unrelated keyboard child builds.
            $target=Join-Path $snapshot 'xorg-server/xkeyboard-config/makefile'
            $source=[IO.File]::ReadAllText($target)
            if($case -eq 'valid-legacy-snapshot'){$source=$source.Replace('wsl ./build.sh','cmd.exe /d /c exit /b 1')}
            $source=$source.Substring(0,$source.IndexOf('SUBDIRS ='))+'all: $(DESTDIR)'+"`n"
            [IO.File]::WriteAllText($target,$source,[Text.UTF8Encoding]::new($false))
            # Sentinel WSL executable catches any accidental implicit/default-guest call.
            $shadow=Join-Path $snapshot 'shadow';New-Item -ItemType Directory $shadow | Out-Null
            Copy-Item -LiteralPath (Join-Path $env:WINDIR 'System32/where.exe') -Destination (Join-Path $shadow 'wsl.exe')
            $env:PATH=$shadow+';'+$env:PATH
            Set-BuildToolEnvironment $snapshot $tools
            Update-BuildSnapshot $snapshot $tools
            Assert ([IO.Path]::GetFullPath((Get-Command wsl.exe).Source) -eq [IO.Path]::GetFullPath((Join-Path $shadow 'wsl.exe'))) 'test did not isolate WSL resolution'
            $result=& $Mhmake -P1 -C (Split-Path $target -Parent) DEBUG=0 MAKESERVER=0 2>&1
            $exitCode=$LASTEXITCODE
            $diagnostic=($result | ForEach-Object { $_.ToString() }) -join "`n"
            $diagnostic | Set-Content (Join-Path $snapshot 'mhmake.log')
            if($case.StartsWith('valid')){Assert ($exitCode -eq 0) 'existing directory with both valid assets rejected'}
            else {
                Assert ($exitCode -ne 0) 'missing or empty keyboard asset accepted'
                $wanted=if($case.StartsWith('missing')){'Missing keyboard asset'}else{'Empty keyboard asset'}
                Assert ($diagnostic.Contains($wanted)) 'failure came from something other than actual asset verification'
            }
            Write-Host "PASS actual mhmake keyboard directory target: $case"
        } catch {$failures++;Write-Host "FAIL actual mhmake keyboard directory target: $case : $_"}
    }
} finally {
    Get-ChildItem Env: | Where-Object {-not $saved.ContainsKey($_.Name)} | ForEach-Object {Remove-Item -LiteralPath ('Env:'+$_.Name)}
    foreach($name in $saved.Keys){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}
    Write-Host "Fixtures: $work"
}
Write-Host "Keyboard target tests: $count; failures: $failures"
if($failures){exit 1}

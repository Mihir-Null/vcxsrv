#Requires -Version 7
param([Parameter(Mandatory)][string]$Mhmake,[Parameter(Mandatory)][string]$ToolPathsFile,[string]$BuildDistro,[string]$BuildUser='Empty')
$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Import-Module (Join-Path $root 'packaging/BuildServer.psm1') -Force
$work=Join-Path $root ('.superpowers/sdd/2026-10-07-vcxsrv-wsl-manager/native-build-tests-'+[guid]::NewGuid())
New-Item -ItemType Directory $work | Out-Null
$toolsRoot=Split-Path $ToolPathsFile -Parent
$saved=@{}; Get-ChildItem Env: | ForEach-Object {$saved[$_.Name]=$_.Value}
$env:PATH='C:/msys64/usr/bin;'+$env:PATH
$inventory=Get-BuildInventory $root (Get-Content $ToolPathsFile -Raw | ConvertFrom-Json -AsHashtable)
$tools=@{}; foreach($tool in $inventory.tools){if($tool.path){$tools[$tool.name]=$tool.path}}
$count=0; $failures=0
function Check($name,[scriptblock]$test){$script:count++;try{& $test;Write-Host "PASS $name"}catch{$script:failures++;Write-Host "FAIL $name : $_"}}
function Assert($condition,$message){if(-not $condition){throw $message}}
function Fixture($name){
    $snapshot=Join-Path $work $name
    foreach($relative in @('tools/mhmake/makebison.bat','tools/mhmake/makelex.bat','xorg-server/hw/xwin/doflexbison.bat','xkbcomp/bison.bat','mesalib/src/compiler/makefile.srcs','mesalib/src/mesa/makefile.srcs','xorg-server/dix/makefile','xorg-server/dix/generate-atoms','xorg-server/dix/BuiltInAtoms','xorg-server/xkeyboard-config/makefile')){
        $destination=Join-Path $snapshot $relative
        New-Item -ItemType Directory (Split-Path $destination -Parent) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $root $relative) -Destination $destination
    };return $snapshot
}
try{
    Check 'selected MSVC linker overrides Unix shadow deterministically' {
        $selection=[ordered]@{shadow='C:/Program Files/Git/usr/bin/link.exe'}
        foreach($name in $tools.Keys){$selection[$name]=$tools[$name]}
        Set-BuildToolEnvironment $root $selection
        Assert ([IO.Path]::GetFullPath((Get-Command link.exe).Source) -eq [IO.Path]::GetFullPath($tools.link)) 'Unix shadow displaced MSVC link'
    }
    Check 'actual mhmake direct and redirected Python retain selected interpreter identity' {
        $selection=$tools.Clone();$selection.python=Join-Path $toolsRoot 'generator fixture/Python With Spaces/Scripts/python.exe'
        Set-BuildToolEnvironment $root $selection
        $fixture=Join-Path $work 'python';New-Item -ItemType Directory $fixture | Out-Null
        @('all: redirected.txt direct.txt','redirected.txt:',("`t"+'$(PYTHON3) -c "import sys; print(sys.executable)" > redirected.txt'),'direct.txt:',("`t"+'$(PYTHON3) -c "import sys; from pathlib import Path; Path(''direct.txt'').write_text(sys.executable)"')) | Set-Content (Join-Path $fixture 'makefile')
        Invoke-CheckedTool $Mhmake @('-P1','-C',$fixture,'DEBUG=0')
        foreach($name in @('redirected.txt','direct.txt')){
            $identity=(Get-Content (Join-Path $fixture $name) -Raw).Trim()
            Assert ([IO.Path]::GetFullPath($identity) -eq [IO.Path]::GetFullPath($selection.python)) 'mhmake used unintended Python'
        }
    }
    Check 'actual mhmake Bison failure halts before real native linker invocation' {
        Set-BuildToolEnvironment $root $tools
        $snapshot=Fixture 'bison';Update-BuildSnapshot $snapshot $tools
        Set-Content (Join-Path $snapshot 'xkbcomp/invalid.y') 'not a valid grammar'
        @('.PHONY: all generate.all link.all','all: link.all','generate.all:',("`t"+'cmd.exe /d /c call "xkbcomp\bison.bat" -d -obad.c invalid.y'),'link.all: generate.all',("`t"+'link.exe /? > linker-called.txt')) | Set-Content (Join-Path $snapshot 'makefile')
        $caught=$null;try{Invoke-CheckedTool $Mhmake @('-P1','-C',$snapshot,'DEBUG=0')}catch{$caught=$_}
        Assert ($null -ne $caught) 'failed Bison returned success'
        Assert (-not(Test-Path (Join-Path $snapshot 'linker-called.txt'))) 'linker ran after failed Bison'
        & $env:ComSpec /d /c "call `"$(Join-Path $snapshot 'xkbcomp/bison.bat')`" -d -obad.c invalid.y" 2>$null
        Assert ($LASTEXITCODE -ne 0) 'batch masked generator status'
    }
    Check 'actual mhmake Mesa generators support selected spaced tool directories' {
        $selection=$tools.Clone();$spaced=Join-Path $work 'Win Flex Bison Tools'
        New-Item -ItemType Directory $spaced | Out-Null
        Copy-Item -Path (Join-Path (Split-Path $tools.win_bison -Parent) '*') -Destination $spaced -Recurse
        $selection.win_bison=Join-Path $spaced 'win_bison.exe';$selection.win_flex=Join-Path $spaced 'win_flex.exe'
        Set-BuildToolEnvironment $root $selection
        $snapshot=Fixture 'mesa';Update-BuildSnapshot $snapshot $selection
        foreach($part in @('compiler','mesa')){
            $fixture=Join-Path $snapshot "mesalib/src/$part"
            Get-Content (Join-Path $fixture 'makefile.srcs') | Where-Object {$_ -match '^(YACC_GEN|LEX_GEN) ='} | Set-Content (Join-Path $fixture 'makefile')
            @('all: grammar.c lexer.c','grammar.c:',("`t"+'$(YACC_GEN) -ogrammar.c grammar.y'),'lexer.c:',("`t"+'$(LEX_GEN) -olexer.c lexer.l')) | Add-Content (Join-Path $fixture 'makefile')
            Set-Content (Join-Path $fixture 'grammar.y') "%token TOKEN`n%%`nstart: TOKEN;`n%%"
            Set-Content (Join-Path $fixture 'lexer.l') "%%`n[a-z]+ return 1;`n%%"
            Invoke-CheckedTool $Mhmake @('-P1','-C',$fixture,'DEBUG=0')
            Assert ((Get-Item (Join-Path $fixture 'grammar.c')).Length -gt 0 -and (Get-Item (Join-Path $fixture 'lexer.c')).Length -gt 0) 'Mesa outputs missing'
        }
    }
    Check 'preflight rejects lxml and mako interpreter without PyYAML' {
        $fixture=Join-Path $work 'no-yaml-env'
        Invoke-CheckedTool $tools.python @('-m','venv','--without-pip',$fixture)
        $python=Join-Path $fixture 'Scripts/python.exe'
        $wheels=@(Get-ChildItem (Join-Path $toolsRoot 'python-wheels') -Filter '*.whl' | Where-Object {$_.Name -match '^(lxml|mako|markupsafe)-'} | Select-Object -ExpandProperty FullName)
        Invoke-CheckedTool $tools.python (@('-m','pip','--python',$python,'install','--no-index')+$wheels)
        & $python -c 'import lxml.etree, mako' 2>$null
        Assert ($LASTEXITCODE -eq 0) 'test fixture lacked lxml or mako'
        $selection=Get-Content $ToolPathsFile -Raw | ConvertFrom-Json -AsHashtable;$selection.python=$python
        $result=Get-BuildInventory $root $selection
        Assert ($result.pythonGenerators -ne 'available' -and -not $result.ready) 'preflight accepted missing PyYAML'
    }
    Check 'empty keyboard directory cannot satisfy assets or trigger guest preparation' {
        $snapshot=Fixture 'stale-keyboard'
        New-Item -ItemType Directory (Join-Path $snapshot 'xorg-server/xkbdata') | Out-Null
        $caught=$null;try{Get-KeyboardAssetManifest $snapshot}catch{$caught=$_}
        Assert ($null -ne $caught) 'empty keyboard assets accepted'
        $caught=$null;try{Initialize-KeyboardAssets $snapshot 'nonexistent-never-probe' 'Empty'}catch{$caught=$_}
        Assert ($caught.Exception.Message -match 'fresh') 'stale directory not rejected before guest execution'
    }
    Check 'actual mhmake generates atoms with explicit native Bash' {
        Set-BuildToolEnvironment $root $tools
        $snapshot=Fixture 'atoms';$fixture=Join-Path $snapshot 'xorg-server/dix'
        @('all: initatoms.c','initatoms.c:',("`t"+'wsl ./generate-atoms BuiltInAtoms $(notdir $@)')) | Set-Content (Join-Path $fixture 'makefile')
        Update-BuildSnapshot $snapshot $tools
        Invoke-CheckedTool $Mhmake @('-P1','-C',$fixture,'DEBUG=0')
        Assert ((Get-Item (Join-Path $fixture 'initatoms.c')).Length -gt 1000) 'native Bash output missing'
    }
    if($BuildDistro){
        Check 'pinned keyboard preparation verifies actual rules and symbols in selected distro' {
            $snapshot=Join-Path $work 'keyboard-build';$destination=Join-Path $snapshot 'xorg-server/xkeyboard-config'
            New-Item -ItemType Directory (Split-Path $destination -Parent) -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $root 'xorg-server/xkeyboard-config') -Destination $destination -Recurse
            $manifest=Initialize-KeyboardAssets $snapshot $BuildDistro $BuildUser
            Assert ($manifest.distro -eq $BuildDistro -and $manifest.user -eq $BuildUser) 'keyboard provenance lost guest selection'
            Assert ($manifest.assets.Count -eq 2) 'rules/base and symbols/us missing'
        }
    }
}finally{
    Get-ChildItem Env: | Where-Object {-not $saved.ContainsKey($_.Name)} | ForEach-Object {Remove-Item -LiteralPath ('Env:'+$_.Name)}
    foreach($name in $saved.Keys){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}
    Write-Host "Fixtures: $work"
}
Write-Host "Round2 tests: $count; failures: $failures"
if($failures){exit 1}

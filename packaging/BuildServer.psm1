Set-StrictMode -Version Latest
function Invoke-CheckedTool {
    param([string]$Path, [string[]]$Arguments)
    & $Path @Arguments | Out-Host
    if ($LASTEXITCODE -ne 0) {
        $failure = [Exception]::new("Build command failed ($LASTEXITCODE): $Path")
        $failure.Data['ExitCode'] = $LASTEXITCODE
        throw $failure
    }
}
function Publish-BuildArtifacts {
    param([string]$BuildRoot, [string]$OutputRoot, [string[]]$Artifacts, [datetime]$StartedUtc, [hashtable]$Manifest)
    if (Test-Path -LiteralPath $OutputRoot) { throw 'OutputRoot must not already exist; use a new staging directory.' }
    $validated = @()
    foreach ($relative in $Artifacts) {
        if ([IO.Path]::IsPathRooted($relative) -or ($relative -split '[\\/]') -contains '..') { throw "Unsafe artifact path: $relative" }
        $path = Join-Path $BuildRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing artifact: $relative" }
        $file = Get-Item -LiteralPath $path
        if ($file.Length -eq 0 -or $file.LastWriteTimeUtc -lt $StartedUtc) { throw "Empty or stale artifact: $relative" }
        $validated += [pscustomobject]@{path=$relative; sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash; bytes=$file.Length}
    }
    if ($validated.Count -eq 0) { throw 'No build artifacts supplied.' }
    New-Item -ItemType Directory -Path $OutputRoot -ErrorAction Stop | Out-Null
    foreach ($artifact in $validated) {
        $destination = Join-Path $OutputRoot $artifact.path
        New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $BuildRoot $artifact.path) -Destination $destination
    }
    $Manifest['artifacts'] = $validated
    $Manifest['finishedUtc'] = [datetime]::UtcNow.ToString('o')
    $Manifest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $OutputRoot 'build-manifest.json') -Encoding utf8
}
function Get-BuildInventory {
    param([string]$SourceRoot, [hashtable]$Overrides = @{})
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vsRoot = $null
    if (Test-Path -LiteralPath $vswhere) {
        $vsRoot = & $vswhere -latest -products '*' -version '[17.0,18.0)' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1
    }
    $candidates = @{}
    if ($vsRoot) {
        $versionFile = Join-Path $vsRoot 'VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt'
        if (Test-Path -LiteralPath $versionFile) {
            $vcVersion = (Get-Content -LiteralPath $versionFile -Raw).Trim()
            $vcBin = Join-Path $vsRoot "VC/Tools/MSVC/$vcVersion/bin/Hostx64/x64"
            foreach ($name in @('cl','nmake','link')) { $candidates[$name] = Join-Path $vcBin "$name.exe" }
        }
        $candidates['msbuild'] = Join-Path $vsRoot 'MSBuild/Current/Bin/MSBuild.exe'
        $candidates['vcvarsall'] = Join-Path $vsRoot 'VC/Auxiliary/Build/vcvarsall.bat'
    }
    $candidates['win_bison'] = 'C:/winflexbison/win_bison.exe'
    $candidates['win_flex'] = 'C:/winflexbison/win_flex.exe'
    $candidates['bash'] = 'C:/Program Files/Git/bin/bash.exe'
    $candidates['mhmake'] = Join-Path $SourceRoot 'tools/mhmake/Release64/mhmake.exe'
    $tools = @()
    foreach ($name in @('git','cl','link','msbuild','nmake','vcvarsall','perl','nasm','jom','python','win_bison','win_flex','bash','gawk','gperf','sed','gzip','mhmake','makensis')) {
        $path = $null
        if ($Overrides.ContainsKey($name)) { $path = [IO.Path]::GetFullPath($Overrides[$name]) }
        elseif ($candidates.ContainsKey($name) -and (Test-Path -LiteralPath $candidates[$name])) { $path = $candidates[$name] }
        else {
            $command = Get-Command "$name.exe" -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($command) { $path = $command.Source }
            elseif (Test-Path -LiteralPath "C:/Program Files/Git/usr/bin/$name.exe") { $path = "C:/Program Files/Git/usr/bin/$name.exe" }
        }
        # System32/WindowsApps bash starts WSL; never probe it during native preflight.
        $safe = $path -and $path -notmatch '(?i)[\\/]Windows[\\/]system32[\\/]bash.exe$|[\\/]WindowsApps[\\/]bash.exe$'
        $exists = $safe -and (Test-Path -LiteralPath $path -PathType Leaf)
        $version = $null; $hash = $null
        if ($exists) {
            $version = (Get-Item -LiteralPath $path).VersionInfo.FileVersion
            $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
            if ($name -in @('git','python','perl','nasm','gawk','gperf','sed','gzip','bash')) {
                $version = (& $path --version 2>&1 | Where-Object { $_.ToString().Trim() } | Select-Object -First 1).ToString()
            }
        }
        $tools += [pscustomobject]@{name=$name; path=$path; status=$(if($exists){'available'}else{'missing'}); version=$version; sha256=$hash; required=($name -notin @('mhmake','makensis'))}
    }
    $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10'
    $sdk = @(Get-ChildItem -LiteralPath (Join-Path $sdkRoot 'Include') -Directory -ErrorAction SilentlyContinue | Where-Object {
        (Test-Path -LiteralPath (Join-Path $_.FullName 'um/Windows.h')) -and (Test-Path -LiteralPath (Join-Path $sdkRoot "Lib/$($_.Name)/um/x64/kernel32.lib"))
    } | Select-Object -ExpandProperty Name)
    $python = $tools | Where-Object name -eq 'python'
    $generators = 'missing-python'
    if ($python.status -eq 'available') {
        & $python.path -c 'import lxml.etree, mako, yaml' 2>$null
        $generators = $(if($LASTEXITCODE -eq 0){'available'}else{'missing-lxml-mako-or-yaml'})
    }
    $perl = $tools | Where-Object name -eq 'perl'
    $nativePerl = 'missing'
    if ($perl.status -eq 'available') {
        $platform = & $perl.path -e 'print $^O' 2>$null
        $nativePerl = $(if ($LASTEXITCODE -eq 0 -and $platform -eq 'MSWin32') {'available'} else {'not-native-win32'})
    }
    return [ordered]@{nativePerl=$nativePerl; schemaVersion=1; sourceRoot=$SourceRoot; visualStudioRoot=$vsRoot; windowsSdkVersions=$sdk; pythonGenerators=$generators; tools=$tools; ready=(@($tools | Where-Object {$_.required -and $_.status -ne 'available'}).Count -eq 0 -and $sdk.Count -gt 0 -and $generators -eq 'available' -and $nativePerl -eq 'available')}
}
Export-ModuleMember -Function Invoke-CheckedTool,Publish-BuildArtifacts,Get-BuildInventory
function Set-BuildToolEnvironment {
    param([string]$BuildRoot, [System.Collections.IDictionary]$Tools)
    $priority = @('python','link','cl','nmake','msbuild','vcvarsall','perl','nasm','jom','win_bison','win_flex','bash','gawk','gperf','sed','gzip','git')
    $directories = @()
    foreach ($name in @($priority) + @($Tools.Keys | Where-Object { $_ -notin $priority -and $_ -notin @('mhmake','makensis') } | Sort-Object)) {
        if ($Tools.Contains($name) -and $Tools[$name]) {
            $leaf = Split-Path $Tools[$name] -Leaf
            if ($leaf -notmatch '^[A-Za-z0-9_.-]+\.(exe|cmd|bat)$') { throw "Unsafe build-tool executable basename: $leaf" }
            $directories += Split-Path ([IO.Path]::GetFullPath($Tools[$name])) -Parent
        }
    }
    $env:PATH = (($directories | Select-Object -Unique) -join ';') + ';' + $env:PATH
    foreach ($name in $priority | Where-Object { $_ -notin @('vcvarsall','git') -and $Tools.Contains($_) -and $Tools[$_] }) {
        $selected = [IO.Path]::GetFullPath($Tools[$name])
        $actual = (Get-Command (Split-Path $selected -Leaf) -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        if (-not [IO.Path]::GetFullPath($actual).Equals($selected, [StringComparison]::OrdinalIgnoreCase)) { throw "Conflicting tool directories would substitute $actual for selected $selected" }
    }
    $env:MHMAKECONF = $BuildRoot
    $env:PYTHON3 = Split-Path $Tools['python'] -Leaf
    $env:IS64 = '1'; $env:DEBUG = '0'; $env:CFLAGS = '-FS'
    $env:LC_ALL = 'C'; $env:LC_CTYPE = 'C'; $env:LANG = 'C'
}
function Update-BuildSnapshot {
    param([string]$BuildRoot, [System.Collections.IDictionary]$Tools)
    foreach ($relative in @('tools/mhmake/makebison.bat','tools/mhmake/makelex.bat','xorg-server/hw/xwin/doflexbison.bat','xkbcomp/bison.bat')) {
        $path = Join-Path $BuildRoot $relative
        $lines = foreach ($line in (Get-Content -LiteralPath $path)) {
            $line.Replace('c:\winflexbison\win_bison.exe','"' + $Tools['win_bison'] + '"').Replace('c:\winflexbison\win_flex.exe','"' + $Tools['win_flex'] + '"').Replace('c:\Python39\python.exe','"' + $Tools['python'] + '"')
            if ($line -match '^c:\\(winflexbison|Python39)\\') { 'if errorlevel 1 exit /b %errorlevel%' }
        }
        Set-Content -LiteralPath $path -Value $lines
    }
    foreach ($relative in @('mesalib/src/compiler/makefile.srcs','mesalib/src/mesa/makefile.srcs')) {
        $path = Join-Path $BuildRoot $relative
        $content = [IO.File]::ReadAllText($path).Replace('c:/winflexbison/win_bison.exe',(Split-Path $Tools['win_bison'] -Leaf)).Replace('c:/winflexbison/win_flex.exe',(Split-Path $Tools['win_flex'] -Leaf))
        [IO.File]::WriteAllText($path,$content,[Text.UTF8Encoding]::new($false))
    }
    $atoms = Join-Path $BuildRoot 'xorg-server/dix/makefile'
    $content = [IO.File]::ReadAllText($atoms).Replace('wsl ./generate-atoms',(Split-Path $Tools['bash'] -Leaf) + ' ./generate-atoms')
    [IO.File]::WriteAllText($atoms,$content,[Text.UTF8Encoding]::new($false))
    $script = Join-Path $BuildRoot 'xorg-server/dix/generate-atoms'
    [IO.File]::WriteAllText($script,[IO.File]::ReadAllText($script).Replace("`r`n","`n"),[Text.UTF8Encoding]::new($false))
    # mhmake can execute an existing directory target again. Verify assets on every execution.
    $verifier = Join-Path $BuildRoot 'packaging/verify-keyboard.bat'
    New-Item -ItemType Directory (Split-Path $verifier -Parent) -Force | Out-Null
    $verifierSource = Join-Path $PSScriptRoot 'verify-keyboard.bat'
    if (-not [IO.Path]::GetFullPath($verifier).Equals([IO.Path]::GetFullPath($verifierSource), [StringComparison]::OrdinalIgnoreCase)) {
        Copy-Item -LiteralPath $verifierSource -Destination $verifier
    }
    $keyboard = Join-Path $BuildRoot 'xorg-server/xkeyboard-config/makefile'
    $verificationCommand = 'cmd.exe /d /c call "' + $verifier + '" "' + $BuildRoot + '"'
    $content = [IO.File]::ReadAllText($keyboard).Replace('wsl ./build.sh',$verificationCommand).Replace('cmd.exe /d /c exit /b 1',$verificationCommand)
    [IO.File]::WriteAllText($keyboard,$content,[Text.UTF8Encoding]::new($false))
}
function Get-KeyboardAssetManifest {
    param([string]$BuildRoot)
    $assets = @()
    foreach ($relative in @('rules/base','symbols/us')) {
        $path = Join-Path $BuildRoot ('xorg-server/xkbdata/' + $relative)
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -eq 0) { throw "Keyboard asset missing or empty: $relative" }
        $assets += @{path=$relative; bytes=(Get-Item -LiteralPath $path).Length; sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}
    }
    return $assets
}
function Initialize-KeyboardAssets {
    param([string]$BuildRoot, [string]$BuildDistro, [string]$BuildUser)
    if (Test-Path -LiteralPath (Join-Path $BuildRoot 'xorg-server/xkbdata')) { throw 'Keyboard preparation requires a fresh snapshot with no existing xkbdata directory.' }
    if ([string]::IsNullOrWhiteSpace($BuildDistro) -or [string]::IsNullOrWhiteSpace($BuildUser)) { throw 'Explicit keyboard build distribution and user are required.' }
    $lock = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'dependencies.lock.json') -Raw | ConvertFrom-Json
    $pin = $lock.keyboardBuild
    $wsl = Join-Path $env:WINDIR 'System32/wsl.exe'
    $guestRoot = & $wsl -d $BuildDistro -u $BuildUser --exec wslpath -a -u ([IO.Path]::GetFullPath($BuildRoot))
    if ($LASTEXITCODE -ne 0) { $failure=[Exception]::new('Selected guest could not resolve build snapshot path.'); $failure.Data['ExitCode']=$LASTEXITCODE; throw $failure }
    Get-Content -LiteralPath (Join-Path $PSScriptRoot 'build-keyboard.sh') -Raw | & $wsl -d $BuildDistro -u $BuildUser --exec bash -s -- $guestRoot $pin.revision $pin.narHash | Out-Host
    if ($LASTEXITCODE -ne 0) { $failure=[Exception]::new('Pinned keyboard preparation failed.'); $failure.Data['ExitCode']=$LASTEXITCODE; throw $failure }
    return @{distro=$BuildDistro; user=$BuildUser; revision=$pin.revision; narHash=$pin.narHash; assets=@(Get-KeyboardAssetManifest $BuildRoot)}
}
Export-ModuleMember -Function Set-BuildToolEnvironment,Update-BuildSnapshot,Get-KeyboardAssetManifest,Initialize-KeyboardAssets

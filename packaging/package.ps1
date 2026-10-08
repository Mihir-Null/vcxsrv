[CmdletBinding()]
param([Parameter(Mandatory)][string]$ServerStage,[Parameter(Mandatory)][string]$ManagerBinary,[Parameter(Mandatory)][string]$Output,[string]$SourceRoot=(Split-Path $PSScriptRoot -Parent),[string]$LicenseInputs,[string]$RuntimeRoot)
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSScriptRoot 'Package.psm1') -Force -DisableNameChecking
$ServerStage=Resolve-InstallPath $ServerStage;$ManagerBinary=Resolve-InstallPath $ManagerBinary;$Output=Resolve-InstallPath $Output;$SourceRoot=Resolve-InstallPath $SourceRoot
if(Test-Path -LiteralPath $Output){throw 'Output must be a new nonexistent directory'}
$server=Get-Content -LiteralPath (Join-OwnedPath $ServerStage 'build-manifest.json') -Raw|ConvertFrom-Json
if($server.status -ne 'built' -or $server.buildMode -ne 'reviewed-source-patch-incremental-full-target' -or !$server.baseSourceCommit -or !$server.sourceCorrection.patchSha256){throw 'Reviewed complete server provenance required'}
$work=Resolve-InstallPath $server.buildWorkspace
Assert-Entries $work $server.artifacts -RequireFiles
Assert-Entries $ServerStage $server.artifacts -RequireFiles
if((Get-FileHash -LiteralPath (Join-OwnedPath $ServerStage $server.sourceCorrection.patch)).Hash -ne $server.sourceCorrection.patchSha256 -or (Get-FileHash -LiteralPath (Join-OwnedPath $work $server.sourceCorrection.path)).Hash -ne $server.sourceCorrection.sourceByteSha256){throw 'Reviewed source correction mismatch'}
$managerMeta=Get-Content -LiteralPath (Join-Path ([IO.Path]::GetDirectoryName($ManagerBinary)) 'manager-build-manifest.json') -Raw|ConvertFrom-Json
if((Get-FileHash -LiteralPath $ManagerBinary).Hash -ne $managerMeta.binary.sha256){throw 'Manager build manifest does not match binary'}
Assert-Entries $SourceRoot @($managerMeta.files|ForEach-Object { $path=Join-OwnedPath $SourceRoot $_.path;[pscustomobject]@{path=$_.path;sha256=$_.sha256;bytes=(Get-Item -LiteralPath $path).Length}}) -RequireFiles
if(!$LicenseInputs){$LicenseInputs=Join-Path ([IO.Path]::GetDirectoryName($ServerStage)) 'license-inputs'}
if(!$RuntimeRoot){$RuntimeRoot=Join-Path $server.toolInventory.visualStudioRoot 'VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT'}
$null=New-Item -ItemType Directory -Path $Output
$map=[ordered]@{
 'vcxsrv.exe'='xorg-server/obj64/servrelease/vcxsrv.exe';'xauth.exe'='apps/xauth/obj64/release/xauth.exe';'xkbcomp.exe'='xkbcomp/obj64/release/xkbcomp.exe';
 'libX11.dll'='libX11/obj64/release/libX11.dll';'libxcb.dll'='libxcb/src/obj64/release/libxcb.dll';'libXau.dll'='libXau/obj64/release/libXau.dll';'libXmu.dll'='libXmu/src/obj64/release/libXmu.dll';'libXext.dll'='libXext/src/obj64/release/libXext.dll';
 'zlib1.dll'='zlib/obj64/release/zlib1.dll';'freetype.dll'='freetype/objs/x64/Release/freetype.dll';'libcrypto-3-x64.dll'='openssl/release64/libcrypto-3-x64.dll';
 'swrast_dri.dll'='mesalib/src/obj64/release/swrast_dri.dll';'swrastwgl_dri.dll'='xorg-server/hw/xwin/swrastwgl_dri/obj64/release/swrastwgl_dri.dll';
 'XKeysymDB'='xorg-server/XKeysymDB';'XErrorDB'='libX11/src/XErrorDB';'Xcms.txt'='libX11/src/xcms/Xcms.txt';'font-dirs'='xorg-server/font-dirs';'protocol.txt'='xorg-server/dix/protocol.txt'
}
foreach($dest in $map.Keys){Copy-Item -LiteralPath (Join-OwnedPath $work $map[$dest]) -Destination (Join-OwnedPath $Output $dest)}
Copy-Item -LiteralPath $ManagerBinary -Destination (Join-OwnedPath $Output 'vcxsrv-wsl-manager.exe')
foreach($name in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll')){Copy-Item -LiteralPath (Join-OwnedPath $RuntimeRoot $name) -Destination (Join-OwnedPath $Output $name)}
foreach($data in @('xkbdata','fonts','locale')){
 foreach($file in Get-ChildItem -LiteralPath (Join-OwnedPath $work ('xorg-server/'+$data)) -File -Recurse){
  if($file.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse data refused'}
  $rel=[IO.Path]::GetRelativePath((Join-Path $work 'xorg-server'),$file.FullName)
  $dest=Join-OwnedPath $Output $rel;$null=New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($dest)) -Force;Copy-Item -LiteralPath $file.FullName -Destination $dest
 }
}
$notices=@('COPYING','xorg-server/COPYING','xkbcomp/COPYING','apps/xauth/COPYING','libX11/COPYING','libxcb/COPYING','libXau/COPYING','libXext/COPYING','libXmu/COPYING','pixman/COPYING','libXdmcp/COPYING','libXfixes/COPYING','libXfont2/COPYING','libfontenc/COPYING','libXinerama/COPYING','libxkbfile/COPYING','libXt/COPYING','zlib/LICENSE','freetype/LICENSE.TXT','freetype/docs/FTL.TXT','freetype/docs/GPLv2.TXT','freetype/src/bdf/README','freetype/src/pcf/README','freetype/src/gzip/zlib.h','freetype/src/autofit/ft-hb.c','freetype/src/autofit/ft-hb.h','openssl/LICENSE.txt','pthreads/LICENSE','pthreads/NOTICE','pthreads/CONTRIBUTORS','mesalib/docs/license.rst','expat/COPYING','xorg-server/xkeyboard-config/COPYING','xorg-server/bitmaps/COPYING','xorg-server/fonts.src/COPYING','xorg-server/fonts.src/font-util/COPYING','xorg-server/fonts.src/encodings/COPYING','xorg-server/fonts.src/misc/COPYING','xorg-server/fonts.src/100dpi/COPYING','xorg-server/fonts.src/100dpi/LU_LEGALNOTICE','xorg-server/fonts.src/75dpi/COPYING','xorg-server/fonts.src/75dpi/LU_LEGALNOTICE','xorg-server/fonts.src/cyrillic/COPYING','xorg-server/fonts.src/cyrillic/COPYRIGHT','xorg-server/fonts.src/OTF/COPYING','xorg-server/fonts.src/OTF/license.txt','xorg-server/fonts.src/Speedo/COPYING','xorg-server/fonts.src/Speedo/COPYRIGHT','xorg-server/fonts.src/TTF/COPYING','xorg-server/fonts.src/TTF/COPYRIGHT.BH','xorg-server/fonts.src/TTF/COPYRIGHT.TXT','xorg-server/fonts.src/Type1/COPYING','xorg-server/fonts.src/Type1/Copyright','xorg-server/fonts.src/Type1/COPYRIGHT.BH','xorg-server/fonts.src/Type1/COPYRIGHT.IBM','xorg-server/fonts.src/terminus-font/README','libregex/src/regex.c','libregex/src/regex_internal.c','libregex/src/regcomp.c','libregex/src/regexec.c','libregex/src/regex_internal.h','libregex/include/regex.h','libregex/src/nl_langinfo.c','libregex/src/langinfo.h')
$notices+=@(Get-ChildItem -LiteralPath (Join-Path $work 'mesalib/licenses') -File | ForEach-Object {'mesalib/licenses/'+$_.Name})
foreach($rel in $notices){$dest=Join-OwnedPath $Output ('notices/'+$rel);$null=New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($dest)) -Force;Copy-Item -LiteralPath (Join-OwnedPath $work $rel) -Destination $dest}
$lgpl=Join-Path $LicenseInputs 'LGPL-2.1.txt';$ofl=Join-Path $LicenseInputs 'OFL-1.1-template.txt'
if((Get-FileHash -LiteralPath $lgpl).Hash -ne '20E50FE7AAE3E56378EBF0417D9DE904F55A0E61E4DF315333E632A4D3555D95' -or (Get-FileHash -LiteralPath $ofl).Hash -ne '1D361A8F8E8CE6E68457DCD93FB56E162E6BAA3BBB7E7573A290D44399F6B57E'){throw 'Official license input checksum mismatch'}
Copy-Item -LiteralPath $lgpl -Destination (Join-OwnedPath $Output 'notices/LGPL-2.1.txt')
$body=Get-Content -LiteralPath $ofl -Raw;$start=$body.IndexOf('SIL OPEN FONT LICENSE');if($start -lt 0){throw 'Official OFL body missing'}
$body.Substring($start)|Set-Content -LiteralPath (Join-OwnedPath $Output 'notices/Terminus-OFL-1.1.txt') -Encoding utf8
Copy-Item -LiteralPath (Join-Path $server.toolInventory.visualStudioRoot 'Licenses/1033/Redist.txt') -Destination (Join-OwnedPath $Output 'notices/MSVC-Redist.txt')
$rc=Get-Content -LiteralPath (Join-OwnedPath $work 'xorg-server/hw/xwin/XWin.rc') -Raw
$serverVersion=([regex]::Match($rc,'VER_PRODUCTVERSION_STR\s+"([^"]+)"')).Groups[1].Value
$version=$managerMeta.version+'-dev.'+$managerMeta.sourceCommit.Substring(0,9)
'# VcXsrv WSL managed product resources; no legacy application launch menu.'|Set-Content -LiteralPath (Join-OwnedPath $Output 'system.XWinrc')
$null=New-Item -ItemType Directory -Path (Join-OwnedPath $Output 'tools')
foreach($name in @('install.ps1','uninstall.ps1','Package.psm1')){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-OwnedPath $Output ('tools/'+$name))}
foreach($name in @('BUILD.md','OPERATIONS.md','READING-ORDER.md')){Copy-Item -LiteralPath (Join-OwnedPath $SourceRoot ('docs/'+$name)) -Destination (Join-OwnedPath $Output $name)}
Copy-Item -LiteralPath (Join-OwnedPath $ServerStage 'build-manifest.json') -Destination (Join-OwnedPath $Output 'server-build-manifest.json')
Copy-Item -LiteralPath (Join-OwnedPath $ServerStage 'source-correction.patch') -Destination (Join-OwnedPath $Output 'source-correction.patch')
Copy-Item -LiteralPath (Join-Path ([IO.Path]::GetDirectoryName($ManagerBinary)) 'manager-build-manifest.json') -Destination (Join-OwnedPath $Output 'manager-build-manifest.json')
$registry=Join-Path ([Environment]::GetFolderPath('UserProfile')) '.cargo/registry/src/index.crates.io-1949cf8c6b5b557f'
$sourceInfo=& python (Join-Path $PSScriptRoot 'source_archive.py') --source $SourceRoot --workspace $work --server-revision $server.sourceCommit --output (Join-OwnedPath $Output 'corresponding-source.zip') --registry $registry
if($LASTEXITCODE -ne 0){throw 'Corresponding source delivery failed'}
$sourceInfo=$sourceInfo|ConvertFrom-Json
@{components=$notices;freetypeLicense='FTL; portions copyright The FreeType Project; full source attribution retained';terminus='Terminus4.49.1 Copyright2020 Dimitar Toshkov Zhekov; actual README plus official OFL body';rustCrates=$sourceInfo.crates;gnuStaticDelivery='complete libregex and work-using-library source/build inputs in corresponding-source.zip; no blanket MIT label';knownNoticeGaps=@('libwinmain and apps/common no standalone notice; corresponding per-file/root source retained');runtimeSource=$RuntimeRoot;scope='local unsigned development handoff, no legal clearance or public release'} | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath (Join-OwnedPath $Output 'component-inventory.json') -Encoding utf8
$files=@(Get-ChildItem -LiteralPath $Output -File -Recurse|ForEach-Object {Get-OwnedEntry $Output ([IO.Path]::GetRelativePath($Output,$_.FullName))})
@{schemaVersion=1;product='VcXsrvWSL';version=$version;serverVersion=$serverVersion;unsignedDevelopment=$true;byteReproducibilityMeasured=$false;server=$server;manager=$managerMeta;files=$files} | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-OwnedPath $Output 'bundle-manifest.json') -Encoding utf8
$null=Read-Bundle $Output
"Complete unsigned development bundle $version at $Output"

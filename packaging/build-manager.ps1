[CmdletBinding()]
param([string]$SourceRoot=(Split-Path $PSScriptRoot -Parent))
$ErrorActionPreference='Stop'
$SourceRoot=[IO.Path]::GetFullPath($SourceRoot)
& cargo build --manifest-path (Join-Path $SourceRoot 'manager/Cargo.toml') --locked --release --bin vcxsrv-wsl-manager
if($LASTEXITCODE -ne 0){throw 'Manager build failed'}
$exe=Join-Path $SourceRoot 'manager/target/release/vcxsrv-wsl-manager.exe'
$commit=& git -C $SourceRoot rev-parse HEAD
if($LASTEXITCODE -ne 0){throw 'Source revision unavailable'}
$files=@(& git -C $SourceRoot ls-files manager | ForEach-Object { $f=Join-Path $SourceRoot $_;[pscustomobject]@{path=$_;sha256=(Get-FileHash -LiteralPath $f).Hash}})
$version=([regex]::Match((Get-Content -LiteralPath (Join-Path $SourceRoot 'manager/Cargo.toml') -Raw),'(?m)^version = "([^"]+)"')).Groups[1].Value
@{schemaVersion=1;sourceCommit=$commit;sourceDirty=([bool](& git -C $SourceRoot status --porcelain -- manager));version=$version;unsignedDevelopment=$true;rustc=(& rustc --version);cargo=(& cargo --version);command='cargo build --locked --release --bin vcxsrv-wsl-manager';files=$files;binary=@{path='vcxsrv-wsl-manager.exe';bytes=(Get-Item -LiteralPath $exe).Length;sha256=(Get-FileHash -LiteralPath $exe).Hash}} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path ([IO.Path]::GetDirectoryName($exe)) 'manager-build-manifest.json') -Encoding utf8
$exe

# Building the VSOCK-capable VcXsrv baseline

The wrapper targets **Windows x64 Release** from source commit `5cd992072b1b94d7d6312f0dab762638ec4ebbe4`. The linked checkout is `C:\Users\walnu\source\vcxsrv-wsl-manager`, branch `feat/wsl-manager`; the preserved primary checkout remains on `pages`. The approved design and plan were carried onto the source baseline in commit `f91eb3e3a`.

## Native preflight

Run from PowerShell 7. Preflight only inspects existing Windows files and invokes native version/module probes. It does not download tools, create OutputRoot, invoke WSL, launch VcXsrv, or modify global PATH.

```powershell
pwsh -NoProfile -File packaging/build-server.ps1 `
  -SourceRoot C:\Users\walnu\source\vcxsrv-wsl-manager `
  -OutputRoot C:\Users\walnu\source\vcxsrv-build\release-001 `
  -Mode Preflight
```

The JSON inventory contains tool paths, versions, installed-file SHA256, SDK versions, Python generator availability, native Perl compatibility and `ready`. Exit 0 means prerequisites are present, exit 2 means prerequisites are missing, and exit 1 means invalid input or an inspection error. Presence does **not** establish compiler compatibility or a successful source build. `mhmake` is optional during preflight because the driver builds it; NSIS is optional because this entry point stages build evidence rather than producing an installer.

To select per-project tools, pass `-ToolPathsFile <json-file>` containing a name-to-absolute-path object, for example `{"nasm":"C:/project-tools/nasm/nasm.exe"}`. An explicitly nonexistent path reports missing and never falls back to PATH. Do not put credentials in this file. Windows System32/WindowsApps Bash launchers are excluded; Git's native Bash is reported separately.

## Observed prerequisites (2026-10-07)

| Component | Observed state |
| --- | --- |
| Visual Studio 2022 BuildTools | Installed, 17.14.41; discovered using installed `vswhere` |
| MSVC toolset / compiler | 14.44.35207 / 19.44.35229.0, x64 |
| MSBuild / NMake / linker | Found through the actual BuildTools root |
| Windows SDK | 10.0.26100.0 headers and x64 kernel32 import library present |
| Python | Native 3.14.7; `lxml.etree` and/or `mako` missing; compatibility with upstream's Python 3.9 generators unverified |
| Perl | Git-bundled 5.42.3 for Cygwin found; **native MSWin32 Perl missing** for OpenSSL's VC-WIN64A configuration |
| Bash / Awk / Sed / Gzip | Existing Git-bundled native executables found |
| NASM / jom / gperf / win_bison / win_flex | Missing |
| mhmake | Source present, executable not built |
| NSIS | Not found; needed separately for an upstream installer |
| Full Cygwin installation | Not established; Git tools do not prove the upstream Cygwin environment exists |

`packaging/dependencies.lock.json` is explicitly **incomplete-no-bootstrap**. It fingerprints installed files, not downloaded archives, and records unresolved dependency selection without inventing versions or hashes. No dependency bootstrap exists. Before implementing one, select immutable official download URLs and versions, download into a project-local directory, independently verify archive SHA256, and record those values. The current build is not fully dependency locked or reproducible yet.

## Checked build entry point

After resolving the prerequisite inventory, select a **new, nonexistent** staging directory:

```powershell
pwsh -NoProfile -File packaging/build-server.ps1 `
  -SourceRoot C:\Users\walnu\source\vcxsrv-wsl-manager `
  -OutputRoot C:\Users\walnu\source\vcxsrv-build\release-001 `
  -Mode Build -Jobs 8 -ToolPathsFile C:\project-tools\vcxsrv-tools.json
```

The wrapper copies tracked inputs (including dirty tracked content) into a new sibling `.work-<GUID>` directory. Ignored prebuilt executables and object directories cannot enter that snapshot. Untracked source additions must be staged in Git before building; this does not require a commit. Deleted tracked inputs cause a copy failure. The original source files remain intact, and the workspace is retained for diagnosis.

`build-driver.ps1` initializes the actual BuildTools x64 environment for its process, substitutes explicit generator paths in the snapshot's batch files, and checks each step: FreeType Rebuild; OpenSSL Configure/jom; pthreads NMake; mhmake Rebuild; server mhmake. Generator batches gain explicit error checks. The driver restores the process environment and current directory even on failure. Child failures preserve their exit code; no `wait` or bare `exit` can conceal a compiler failure.

After a successful command chain, the wrapper validates these newly generated, nonempty files and their timestamps **before creating staging output**:

- `xorg-server/obj64/servrelease/vcxsrv.exe`
- `apps/xauth/obj64/release/xauth.exe`
- `xkbcomp/obj64/release/xkbcomp.exe`

Their relative paths are preserved. `build-manifest.json` records the source commit, dirty state, tool inventory, command, workspace, timestamps, artifact lengths and SHA256. This is a baseline compilation evidence bundle; it is **not a deployable complete VcXsrv distribution**. Upstream NSIS packaging also includes Mesa, XML and X11 DLLs, runtime libraries, fonts and data. Installer/distribution work must retain those requirements and account for tracked, prebuilt libxml2 DLLs separately from freshly compiled artifacts.

## Validation and pending acceptance

```powershell
pwsh -NoProfile -File packaging/tests/build-server.tests.ps1
```

Six native behavioral checks exercise the CLI and helpers: missing explicit tool and artifact-free preflight; exact child exit propagation; rejection of stale artifacts; independently known SHA256 and source provenance; traversal rejection; and actual build-driver early failure with environment/location restoration. Tests use synthetic text artifacts and a compiler process returning 19; **they do not compile or validate an X server**.

Full baseline compilation remains **NOT RUN** due to the observed missing toolchain. The earlier guest hold was lifted, but this task made no guest changes or WSL calls and did not launch servers. Once project-local dependencies are verified, run preflight and Build, retain the resulting manifest, then separately verify the complete server and installation runtime. Neither a prebuilt binary nor the synthetic test fixtures satisfy source-build acceptance.

## Upstream references and rationale

- `origin`: https://github.com/Mihir-Null/vcxsrv.git
- `upstream`: https://github.com/marchaesen/vcxsrv.git
- `fork-upstream`: https://github.com/vivimillin/vcxsrv.git
- `pr-upstream`: https://github.com/vivimillin/vcxsrv-upstream.git

`git ls-remote --heads origin` verified the requested imports: master `d0a1eaf7ee15fcdf4f683388a88fec49078e6408`; released `3244d44e08ca6a350b8caad28e47b2fc90bcf59c`; feature/raw-input-mouse `8e100944fcdc798841a226daa7c8527eae650ef5`; feature/vsock-wsl2 `5cd992072b1b94d7d6312f0dab762638ec4ebbe4`; feature/vsock-wsl2-pr `a699804982d96c4d534e4f0d2f9671ac98203daa`. Local origin tracking refs need a later explicit fetch if they are wanted locally. No source or documentation history was replaced.

The [upstream README](https://github.com/marchaesen/vcxsrv) and local `HOW_TO_BUILD.txt`/Dockerfile identify the supported MSVC build family. Upstream currently assumes a WSL terminal and hardcodes Community, WinFlexBison and Python39 paths. [Microsoft's command-line guidance](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170) recommends the compiler environment scripts, and [vswhere guidance](https://github.com/microsoft/vswhere/wiki) documents discovery instead of edition-specific guesses. This wrapper follows those discovery practices; its complete native recipe remains unverified until a real compile.

Existing `buildall.sh`, `setenv.py`, `setenv.bat`, `setenv.sh`, Docker recipe and NSIS scripts remain reference material. In particular, do not use legacy `buildall.sh` as proof of success: its error handler uses a bare `exit`, and its mhmake MSBuild branches call `wait` before checking the compiler result. The Windows-container recipe requires Windows containers; Linux Docker cannot execute it.

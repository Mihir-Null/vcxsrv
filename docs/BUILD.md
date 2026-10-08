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

## Initial inventory before project-local provisioning (2026-10-07)

| Component | Observed state |
| --- | --- |
| Visual Studio 2022 BuildTools | Installed, 17.14.41; discovered using installed `vswhere` |
| MSVC toolset / compiler | 14.44.35207 / 19.44.35229.0, x64 |
| MSBuild / NMake / linker | Found through the actual BuildTools root |
| Windows SDK | 10.0.26100.0 headers and x64 kernel32 import library present |
| Python | Native 3.14.7; `lxml.etree`, `mako` and/or `yaml` missing; compatibility with upstream's Python 3.9 generators unverified |
| Perl | Git-bundled 5.42.3 for Cygwin found; **native MSWin32 Perl missing** for OpenSSL's VC-WIN64A configuration |
| Bash / Awk / Sed / Gzip | Existing Git-bundled native executables found |
| NASM / jom / gperf / win_bison / win_flex | Missing |
| mhmake | Source present, executable not built |
| NSIS | Not found; needed separately for an upstream installer |
| Full Cygwin installation | Not established; Git tools do not prove the upstream Cygwin environment exists |

The initial lock captured installed-file fingerprints only. The subsequent provisioning section below records the verified fixed archives and Python wheels now in `packaging/dependencies.lock.json`; no automatic bootstrap or machine-independent runtime bundle is implemented.

## Checked build entry point

After resolving the prerequisite inventory, select a **new, nonexistent** staging directory:

```powershell
pwsh -NoProfile -File packaging/build-server.ps1 `
  -SourceRoot C:\Users\walnu\source\vcxsrv-wsl-manager `
  -OutputRoot C:\Users\walnu\source\vcxsrv-build\release-001 `
  -Mode Build -Jobs 8 -ToolPathsFile C:\project-tools\vcxsrv-tools.json `
  -BuildDistro NixOS -BuildUser Empty
```

The wrapper copies tracked inputs (including dirty tracked content) into a new sibling `.work-<GUID>` directory. Ignored prebuilt executables and object directories cannot enter that snapshot. Untracked source additions must be staged in Git before building; this does not require a commit. Deleted tracked inputs cause a copy failure. The original source files remain intact, and the workspace is retained for diagnosis.

`build-driver.ps1` initializes the actual BuildTools x64 environment for its process, substitutes explicit generator paths in the snapshot's batch files, and checks each step: FreeType Rebuild; OpenSSL Configure/jom; pthreads NMake; mhmake Rebuild; explicit keyboard data preparation; server mhmake. Generator batches gain explicit error checks. Tool directory precedence is deterministic, and the selected compiler/linker cannot be shadowed by Unix utilities. The selected Python directory is first and PYTHON3 uses its validated executable basename; this avoids an actual mhmake quoted-command/redirection parser defect while validating exact selected interpreter resolution. Conflicting command-text tool selections fail explicitly. Git provenance commands are invoked by their selected absolute path. The driver restores the process environment and current directory even on failure. Child failures preserve their exit code; no `wait` or bare `exit` can conceal a compiler failure.

After a successful command chain, the wrapper validates these newly generated, nonempty files and their timestamps **before creating staging output**:

- `xorg-server/obj64/servrelease/vcxsrv.exe`
- `apps/xauth/obj64/release/xauth.exe`
- `xkbcomp/obj64/release/xkbcomp.exe`

Their relative paths are preserved. `build-manifest.json` records the source commit, dirty state, tool inventory, command, workspace, timestamps, artifact lengths and SHA256. This is a baseline compilation evidence bundle; it is **not a deployable complete VcXsrv distribution**. Upstream NSIS packaging also includes Mesa, XML and X11 DLLs, runtime libraries, fonts and data. Installer/distribution work must retain those requirements and account for tracked, prebuilt libxml2 DLLs separately from freshly compiled artifacts.

## Validation and pending acceptance

```powershell
pwsh -NoProfile -File packaging/tests/build-server.tests.ps1
```

Nine native behavioral checks exercise the CLI and helpers: missing explicit tool and artifact-free preflight; exact child exit propagation; rejection of stale artifacts; independently known SHA256 and source provenance; traversal rejection; and actual build-driver early failure with environment/location restoration. Tests use synthetic text artifacts and a compiler process returning 19; **they do not compile or validate an X server**.

The original source baseline compiled successfully with controller diagnostic workarounds: server, xauth, xkbcomp, keyboard rules/symbols and a valid generated PCF font were verified. Those results establish the baseline can build on this machine. They do **not** establish a pristine full wrapper run or final managed-server/runtime acceptance. The production wrapper incorporates the observed fixes and has focused native/guest checks; the controller will run a fresh complete build and preserve its manifest. No server launch or NixOS activation is part of these wrapper tests.

## Upstream references and rationale

- `origin`: https://github.com/Mihir-Null/vcxsrv.git
- `upstream`: https://github.com/marchaesen/vcxsrv.git
- `fork-upstream`: https://github.com/vivimillin/vcxsrv.git
- `pr-upstream`: https://github.com/vivimillin/vcxsrv-upstream.git

`git ls-remote --heads origin` verified the requested imports: master `d0a1eaf7ee15fcdf4f683388a88fec49078e6408`; released `3244d44e08ca6a350b8caad28e47b2fc90bcf59c`; feature/raw-input-mouse `8e100944fcdc798841a226daa7c8527eae650ef5`; feature/vsock-wsl2 `5cd992072b1b94d7d6312f0dab762638ec4ebbe4`; feature/vsock-wsl2-pr `a699804982d96c4d534e4f0d2f9671ac98203daa`. Local origin tracking refs need a later explicit fetch if they are wanted locally. No source or documentation history was replaced.

The [upstream README](https://github.com/marchaesen/vcxsrv) and local `HOW_TO_BUILD.txt`/Dockerfile identify the supported MSVC build family. Upstream currently assumes a WSL terminal and hardcodes Community, WinFlexBison and Python39 paths. [Microsoft's command-line guidance](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170) recommends the compiler environment scripts, and [vswhere guidance](https://github.com/microsoft/vswhere/wiki) documents discovery instead of edition-specific guesses. This wrapper follows those discovery practices; its complete native recipe remains unverified until a real compile.

Existing `buildall.sh`, `setenv.py`, `setenv.bat`, `setenv.sh`, Docker recipe and NSIS scripts remain reference material. In particular, do not use legacy `buildall.sh` as proof of success: its error handler uses a bare `exit`, and its mhmake MSBuild branches call `wait` before checking the compiler result. The Windows-container recipe requires Windows containers; Linux Docker cannot execute it.

## Isolated tool provisioning verified on 2026-10-07

The dependency lock now records fixed official archives for Strawberry Perl 5.42.3.1, NASM 2.16.03, jom 1.1.4, WinFlexBison 2.5.25 and MSYS2 gperf 3.3-1, plus Python wheels for lxml 6.1.3, Mako 1.4.3 and MarkupSafe 3.0.4 and PyYAML 6.0.3. Perl/jom/Python/gperf archive hashes were compared with publisher metadata. NASM and WinFlexBison hashes fingerprint the official HTTPS downloads; no separate publisher digest was available, and these entries explicitly say so.

For this development build, tools live outside the checkout under `C:/Users/walnu/source/vcxsrv-build-tools`. Python packages are installed in that directory's virtual environment, never globally. gperf uses the already installed `C:/msys64/usr/bin` runtime on this host; add that directory to the build process PATH only. Native Perl needs the build process locale set to `C`, because it does not accept the inherited `C.UTF-8` locale here. No registry or global PATH modifications were made. Full portable bootstrap still needs to include or verify the MSYS runtime dependency before it can be advertised as machine-independent.

The provisioned native preflight returns `ready: true` when the MSYS runtime directory is present in the build process PATH. Preflight also requires `yaml`, `lxml.etree` and `mako`; it does not start a guest. This establishes tool availability, not complete build compatibility. Review fixes are validated against actual built mhmake rather than an emulated command parser.

## Selected guest and pinned keyboard assets

`-BuildDistro` defaults explicitly to `NixOS`, and `-BuildUser` to `Empty`; pass both to select another supported build guest. Every guest command uses those names. The wrapper never relies on WSL's default distribution. Only keyboard data uses the guest; atom generation uses the explicitly selected native Bash executable.

The lock pins nixpkgs revision `ecc58f32d1069a3e3f8a991e2233d365689f748d` and narHash `sha256-vrwAiXmz+0hs/IWXZalFG/Ws6xQRcW+gClUDlfvcfjQ=`. `build-keyboard.sh` uses Nix fetchTree with that content hash and x86_64-linux to obtain Meson, Ninja, gettext, Perl and Python3. The existing guest must supply Nix, Bash and basic bootstrap utilities. The script sets process-only bootstrap PATH before invoking the pinned packages, so inherited Windows PATH/WSLENV cannot hide guest utilities. No guest configuration activation occurs.

Keyboard generation requires a fresh snapshot without `xorg-server/xkbdata`. It builds/installs into a new temporary staging prefix, verifies nonempty `rules/base` and `symbols/us`, and moves the validated data into place only after every step succeeds. A stale or partially created xkbdata directory is rejected, even if a previous upstream target might have accepted its existence. Failed build/staging directories are retained for diagnosis. The snapshot's implicit `wsl ./build.sh` recipe is disabled; missing preparation cannot switch guests silently.

The build manifest includes selected distro/user, pinned revision/narHash and hashes/sizes of those two essential keyboard assets. The complete generated data tree remains in the retained build workspace; the three-executable evidence bundle is still not a complete runtime distribution.

## Focused actual-tool regressions

```powershell
pwsh -NoProfile -File packaging/tests/build-server-native.tests.ps1 `
  -Mhmake C:\path\to\built\mhmake.exe `
  -ToolPathsFile C:\project-tools\vcxsrv-tools.json `
  -BuildDistro NixOS -BuildUser Empty
```

The native suite requires the provisioned tools tree beside the tool paths file, including the spaced-path Python fixture and the verified Python wheels. It executes real mhmake for direct/redirected Python interpreter identity, failed xkbcomp Bison generation before any native linker call, both Mesa Bison/Flex definitions using selected paths with spaces, and native Bash atoms generation. A real clean Python environment with only lxml/Mako/MarkupSafe verifies missing PyYAML rejection. It also checks MSVC link precedence and stale keyboard rejection. Passing BuildDistro additionally runs the focused real pinned keyboard build; omitting it leaves that guest check unrun. Small ignored fixtures are retained under the SDD directory for review.

All generator substitutions occur only in the fresh snapshot: four batch files including `xkbcomp/bison.bat`, both Mesa `makefile.srcs`, and atoms/keyboard recipes. Canonical upstream build files remain unchanged. Unsafe executable basenames and conflicting selected generator directories fail explicitly; directory spaces are supported by selecting basenames after exact path resolution checks.

The pinning and setup/compile/install workflow follow [Nix fetchTree's hash contract](https://nix.dev/manual/nix/2.26/language/builtins) and [Meson's documented commands](https://mesonbuild.com/Commands.html).

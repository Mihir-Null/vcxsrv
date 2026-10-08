# VcXsrv WSL Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an authenticated VSOCK VcXsrv distribution with a native Windows manager and declarative McQueenix integration, and validate it on the user's machine once their rebuild is finished.

**Architecture:** Keep the server patch, manager, Linux integration and packaging independently testable in one repository. The Windows manager owns server lifecycle and authority credentials; a NixOS user service supplies the Linux socket relay. Readiness requires an authenticated X11 exchange, not merely a running process.

**Tech Stack:** Existing VcXsrv C/C++ and MSVC build; Rust stable on x86_64-pc-windows-msvc, Win32 APIs via windows 0.62, serde/serde_json; NixOS/Home Manager, systemd and socat; PowerShell packaging. Commit Cargo.lock and record the actual compiler/dependency versions used.

**Spec:** ../specs/2026-10-07-vcxsrv-wsl-manager-design.md (approved by the user on 2026-10-07).

## Global Constraints

- Initial supported target: Windows 11 x64, Store WSL2, distribution NixOS, user Empty, host mcqueenix.
- Default display 1 / VSOCK port 106001. Distro, user, display and launch executable/arguments are configurable.
- Native Wayland, audio, SSH forwarding and a general application store/browser are outside the first release.
- No WebView or Python runtime for the manager; no GWSL code or assets copied.
- No TCP fallback, firewall exception, new Windows service or Hyper-V group membership. Require cookie authentication; exact VM binding is not user/distro isolation.
- No secret in arguments, logs, source control or the Nix store. Windows user-restricted ACL; Linux mode 0600.
- Preserve upstream history, pages, existing configuration and uncommitted work. No force push, default-branch change or public release.
- LIVE HOLD lifted by the user on 2026-10-07: new WSL is online. Read-only inspection, builds and non-disruptive tests are authorized. Continue to coordinate shutdown/restart or loss of existing Emacs buffers.
- Once the hold is lifted, separately coordinate actions that destroy guest processes or could lose unsaved buffers. Do not interpret permission to inspect/test as permission to discard work.
- Keep a source build outstanding if only a prebuilt binary is tested. Report failed and not-run tests distinctly.

## Review Focus

- Internal clipboard/multiwindow clients under no-TCP plus strict authentication must keep working (Tasks 2 and 9).
- Distro names with spaces/Unicode, UTF-16 output and localized Windows must not select the wrong guest (Tasks 3 and 4).
- Manager crash/relaunch and PID reuse must not kill or impersonate an unrelated server or rotate credentials under live clients (Tasks 5 and 8).
- Missing/stale credentials, symlinked authority paths and occupied X sockets must fail safely with a useful message (Tasks 6 and 7).
- Polling and retries must not keep an idle WSL instance alive or start a stopped distro (Tasks 3, 5 and 9).

## Structure and dependency map

- Existing server: xorg-server/hw/xwin/{winprocarg.c,winglobals.c,winglobals.h,win.h,winvsock.c,InitOutput.c,windisplay.c,winauth.c}; xorg-server/os/connection.c; X11/xtrans/Xtranssock.c; xorg-server/hw/xwin/man/XWin.man.
- Native tests: tests/server/{managed-profile.c,discovery.c} and tests/acceptance/{x11-probe.py,Test-Server.ps1,Test-Package.ps1}.
- Manager: manager/{Cargo.toml,Cargo.lock,build.rs,app.manifest}; manager/src/{main.rs,lib.rs,config.rs,process.rs,wsl.rs,authority.rs,controller.rs,tray.rs,diagnostics.rs}; manager/tests/{config.rs,process.rs,authority.rs,controller.rs}.
- Nix integration: integration/nixos/{default.nix,home-manager.nix,helper.py,tests.nix,README.md}.
- Packaging: packaging/{build-server.ps1,build-manager.ps1,package.ps1,install.ps1,uninstall.ps1,dependencies.lock.json}; docs/{BUILD.md,OPERATIONS.md,VALIDATION.md}.
- Tasks 1-3 produce a testable server; Tasks 4-6 produce a testable standalone manager; Task 7 produces guest integration; Tasks 8-9 assemble and validate the product. Manager work can proceed if the full server build is waiting for WSL availability, but server completion may not be skipped.

## Task 1: Establish implementation baseline and reproducible build entry point

**Files:** packaging/build-server.ps1, packaging/dependencies.lock.json, docs/BUILD.md; preserve existing buildall.sh, setenv.py and installer packaging as references.
**Interfaces:** build-server.ps1 -SourceRoot <path> -OutputRoot <path> -Mode Preflight|Build emits a tool inventory or builds into the requested staging directory. A build manifest records source commit, dirty state, tool versions, artifact hashes and build command.

- [ ] At execution time read using-git-worktrees and repository instructions. Use an isolated checkout based on 5cd992072b1b94d7d6312f0dab762638ec4ebbe4; bring the approved spec and plan across without replacing baseline source with pages. Verify winvsock.c exists, record both upstream remotes and preserve the local pages documentation commits. The user requested upstream branch import: origin now also has master, released, feature/raw-input-mouse, feature/vsock-wsl2 and feature/vsock-wsl2-pr; verify these references at execution time.
- [ ] Run a build preflight using existing Windows tools only while LIVE HOLD applies. Inventory MSVC, Windows SDK, mhmake, Perl, NASM, build-shell tools and Python generators. Report missing dependencies instead of installing unpinned arbitrary versions.
- [ ] Pin selected dependency versions, official download URLs and verified hashes in dependencies.lock.json before adding a bootstrap step. Use per-project tools where practical; do not change global PATH or rely on the legacy Windows-container recipe running in Linux Docker.
- [ ] Add a build wrapper that resolves paths explicitly, rejects missing tools, propagates nonzero exits and stages only freshly built artifacts. Existing buildall.sh error paths must not turn a failed compiler command into success.
- [ ] Verify Preflight returns nonzero with a deliberately nonexistent tool path and emits no artifacts. After the guest hold is lifted, use the documented WSL/MSVC path where required; validate the complete baseline source build before counting Task 1 complete.
- [ ] Commit the baseline/build documentation and wrapper. Keep the build limitation recorded if the live hold is still active; continue independent manager tasks.

## Task 2: Add explicit managed profile and enforce authentication

**Files:** winprocarg.c, winglobals.{c,h}, InitOutput.c, win.h, connection.c, XWin.man; tests/server/managed-profile.c; tests/acceptance/{x11-probe.py,Test-Server.ps1}. Inspect windisplay.c, winauth.c and local transport code before changing them.
**Interfaces:** vcxsrv.exe :1 -wslmanaged -wsldistro NixOS -auth <authority-path>. -wslmanaged enables multiwindow, clipboard and automatic VSOCK, forbids TCP and unauthenticated host fallback. Existing generic invocation semantics remain intact. Missing authority or conflicting flags produce nonzero startup exit and an explanation.

- [ ] Add failing native tests asserting `managed && no_auth_path -> reject`, `managed && defeat_access_control -> reject`, `managed && tcp_requested -> reject`, and `managed && failed_cookie && hyperv_peer -> reject`. Check both argument orders; -listen tcp or -ac later in the command line cannot undo the profile.
- [ ] Run the native test harness and record the expected failures against the unmodified baseline.
- [ ] Implement the profile validation after argument parsing and before listener creation. In ClientAuthorized, require successful authorization for Hyper-V peers in managed mode even if InvalidHost would accept them; valid internal generated-cookie authentication must still succeed. Do not log cookies.
- [ ] Add an X11 probe that sends a real setup request and returns success/failure with a bounded read. Its expected live outcomes are no cookie rejected, wrong cookie rejected, valid cookie accepted. Pass credentials via an authority file/stdin, not command arguments. Keep the Python probe a development test dependency, not a Windows manager runtime dependency.
- [ ] Verify how multiwindow and clipboard internal clients connect with TCP disabled. Preserve a supported local transport and generated internal credentials. Source inspection confirms LOCALCONN and UNIXCONN disabled. User explicitly approved adding and testing a Windows-local transport. Implement native AF_UNIX via Winsock for internal clients, without TCP fallback; test actual local connections, strict auth, cleanup and multiwindow/clipboard call sites. Expand this task into a transport subtask if needed; no repeat design approval required.
- [ ] Run native tests, build the server and verify new help/options. Defer real VSOCK protocol tests to Task 9 under LIVE HOLD. Commit only when source tests pass; record live checks as pending.

## Task 2c: Show managed native windows after initial hints are ready

Live acceptance demonstrated that VcXsrv emits CREATE/SHOW while its Emacs frame is still a TOOLWINDOW. LeopardWM rejects those events; later APPWINDOW promotion emits only location events, so manual refresh is required. Correct this in the managed server profile before counting tiler integration complete.

- [ ] Prepare managed ordinary windows while hidden, queue existing asynchronous WM initialization, then perform first real ShowWindow after final name/style/icon/state. Preserve generic and override-redirect behavior.
- [ ] Keep per-window state on the owner thread and carry copied XID plus generation/token for readiness. Invalidate on unmap/destroy, reject stale completions, and prevent duplicate map preparation on the completion ShowWindow callback.
- [ ] Cover early visibility from UpdateState and fullscreen/no-decoration hints; preserve initial minimized/maximized states, owned dialogs, skip-taskbar/tool windows and InputOnly behavior. Do not synthesize duplicate SHOW events or make manager polling/Leopard refresh the product workaround.
- [ ] Test real native event ordering and map/unmap/remap races; compile affected units/full server. Verify new separate Emacs frames automatically tile without refresh, then retain resize/focus and user-verified rendering/input behavior.
## Task 3: Make discovery explicit, Unicode-safe and bounded

**Files:** winvsock.c, winprocarg.c, winglobals.{c,h}, win.h, XWin.man; tests/server/discovery.c.
**Interfaces:** -wsldistro <name> selects the discovery distro. winWslQueryRunningDistro and winWslQueryVmId return success, stopped, missing, unsupported or timeout without changing listener state on invalid data. Only the server main thread replaces listeners.

- [ ] Write failing parser/process tests for UTF-16LE quiet output, CRLF, BOM/no BOM, NixOS, a name containing spaces/non-ASCII, invalid GUID, oversized/truncated output and timed-out child. Assert a stopped selected distro never produces a guest command.
- [ ] Verify failures, then replace English Running parsing with `wsl.exe --list --running --quiet` and exact selected-name matching. Resolve the trusted system wsl.exe path; create hidden Unicode processes with argument escaping, bounded output and a 5-second timeout.
- [ ] Keep existing worker/main-thread separation and VM listener rebinding. Re-check running membership before issuing guest commands, acknowledge the query/start race, and avoid guest polling once bound. Generic auto-selection must still exclude WSL1 or fail conservatively when version cannot be established.
- [ ] Run native tests. Add a fake-runner lifecycle test: bound state produces zero guest commands over repeated polls; disappearance cancels stale state; a new valid VM ID becomes a main-thread rebind request.
- [ ] Build and commit. Live restart/idle behavior remains pending Task 9; do not claim that rebinding preserves clients whose guest was shut down.

## Task 4: Implement manager configuration and process boundary

**Files:** manager/Cargo.toml, Cargo.lock, src/{lib.rs,config.rs,process.rs,wsl.rs}, tests/{config.rs,process.rs}.
**Interfaces:** Config { version: u32, distro: String, user: String, display: u16, server_path: PathBuf, launcher: LaunchSpec }; LaunchSpec { program: String, args: Vec<String> }; Config::validate() -> Result<(), ConfigError>; Config::vsock_port() -> u32. CommandSpec { program: PathBuf, args: Vec<OsString>, timeout: Duration }; ProcessRunner::run(spec, stdin: Option<&[u8]>) -> Result<CommandOutput, ProcessError>. WslClient::running_distros() never executes inside a guest.

- [ ] Add tests asserting defaults are version 1, NixOS, Empty, display 1, port 106001, launcher emacsclient with [-c,-n]; reject unknown schema, empty names, control characters, display > 99 and relative server paths after default-path resolution.
- [ ] Add process tests using a fixture executable that echoes its received argument vector: Unicode, spaces, quotes and shell metacharacters survive as literal arguments. Test timeout, output limit, nonzero exit and redacted diagnostics for stdin.
- [ ] Run `cargo test --manifest-path manager/Cargo.toml` to observe failures, then implement configuration and hidden bounded process execution. Use std::process::Command/Win32 argument passing; no cmd /c or shell string assembly. Defaults locate the companion server relative to the installed manager.
- [ ] Run tests and `cargo clippy --manifest-path manager/Cargo.toml --all-targets -- -D warnings`; commit the working core with locked dependencies.

## Task 5: Implement ownership, lifecycle and readiness state machine

**Files:** src/{controller.rs,diagnostics.rs}, tests/controller.rs.
**Interfaces:** State = WaitingForWsl | Connecting | Ready | Error(String); Action = StartServer | Launch | Refresh | Detach | StopServerConfirmed. Controller::dispatch(Action) -> Vec<Effect>; Controller::observe(Event) -> Vec<Effect>. Effects run on a worker using ProcessRunner; the tray observes states without blocking.

- [ ] Add state tests asserting passive start with stopped WSL remains WaitingForWsl and emits no guest execution; Launch may start only the configured guest; socket/process presence alone cannot yield Ready; authenticated probe success is required.
- [ ] Test repeated StartServer produces one child; unowned PID is never terminated; controller recovery after manager crash validates executable identity, user SID, process creation time and protected ownership record. Stale PID/credentials yield Error rather than process killing or cookie rotation.
- [ ] Implement the pure state machine and Windows child handle ownership. Use a SID-scoped single-instance mutex. On ordinary exit detach from the server; StopServerConfirmed is the only user action that destroys an owned live server. Recoverable server ownership survives manager restart through protected metadata, not a bare PID.
- [ ] Bound bootstrap/readiness retries to 15 seconds per explicit connection attempt. Do not continuously spawn guest probes in Ready; report it as the last authenticated state and invalidate on server/VM changes. Refresh may probe only an already-running selected guest; handle shutdown races honestly.
- [ ] Run controller tests, clippy and release build. Commit.

## Task 6: Implement authority files and native tray manager

**Files:** src/{authority.rs,tray.rs,main.rs}, app.manifest, build.rs, tests/authority.rs.
**Interfaces:** AuthorityStore::load_or_create(display: u16) -> Result<Authority, AuthorityError>; Authority::encode_xauthority() -> Vec<u8>; cookie bytes have no Debug/Display output. Tray actions dispatch Task 5 actions; bootstrap uses stdin and fixed helper verbs from Task 7.

- [ ] Add byte-level Xauthority tests: big-endian lengths, MIT-MAGIC-COOKIE-1, 16-byte cookie, configured display, truncated input rejected. Test wrong-owner/reparse-point authority paths are rejected and an existing server's cookie is reused.
- [ ] Implement cryptographic generation with BCryptGenRandom, atomic writes and restrictive Windows ACLs. Metadata and logs never contain cookie bytes. Verify permissions with the Windows security APIs rather than assuming a hidden directory is private.
- [ ] Implement a native Win32 tray/message loop with status, Start server, Launch Emacs, diagnostics, open config/logs, startup toggle, Exit manager and separately confirmed Stop server. Use a worker channel for long operations; duplicate clicks do not enqueue unbounded launches. Restore the icon after Explorer's TaskbarCreated notification.
- [ ] Provide CLI `doctor --json` for diagnostics and `--tray` for normal operation using the same controller. Missing companion executable or helper produces actionable Error text. A manager-only process is never advertised as a ready display.
- [ ] Run tests, clippy and release build; manually smoke-test tray responsiveness, missing-server handling, single-instance behavior and detach on Windows without invoking WSL during LIVE HOLD. Commit.

## Task 7: Add declarative guest relay and kittymacs integration

**Files:** integration/nixos/{default.nix,home-manager.nix,helper.py,tests.nix,README.md}; after LIVE HOLD is lifted, inspect /home/Empty/src/EmptyNet/wiki/{mcqueenix.org,wsl.org,emacs.org} and the actual kittymacs module before changing its authoritative Org source.
**Interfaces:** services.vcxsrvWsl { enable; user = "Empty"; display = 1; port = 106001; }; home-manager.users.Empty.vcxsrvWsl { enable; display = 1; }. Expose fixed helper `/run/current-system/sw/bin/vcxsrv-wsl` with `bootstrap`, `probe`, and `launch -- <executable> <args...>` verbs. Bootstrap consumes exactly one bounded binary Xauthority record on stdin; it never reads a secret from argv.

- [ ] Add module/guest tests asserting one relay service, TCP absent, socket owned by the target user with mode 0600, authority mode 0600, display/port consistency, missing user/config rejected, stale foreign socket not deleted and symlinked credential path rejected. Do not start a Nix VM test during the user's rebuild; write tests now and run later or in an explicitly available isolated environment.
- [ ] Implement a user relay service managed by systemd and socat using a private runtime socket, e.g. UNIX-LISTEN:/run/user/1000/vcxsrv-wsl/X1 with VSOCK-CONNECT:2:106001. Set DISPLAY=unix:/run/user/1000/vcxsrv-wsl/X1 (no trailing :1). Actual XOpenDisplay routing is verified; libxcb uses authority display selector 0 for a socket path, so encode the guest record with number 0 and preserve the cookie. Verify real successful cookie selection in acceptance. Preserve WSLg's read-only /tmp/.X11-unix mount. Derive runtime paths from the configured user, never hardcode UID 1000.
- [ ] Store authority at the user's runtime directory under vcxsrv-wsl; bootstrap creates it atomically with safe ownership and no symlink following. Probe performs a bounded actual authenticated X11 setup through the relay, including a direct VSOCK negative-test mode for development so socket permissions cannot mask server authentication failures.
- [ ] Configure the real kittymacs daemon unit's DISPLAY and XAUTHORITY after inspecting its definition; select pkgs.emacs, and order after the relay. Avoid changing the daemon to a Wayland backend. The launcher waits for authenticated readiness, then execs emacsclient -c -n with explicit environment and without shell interpolation. A daemon may start headlessly before the Windows server; frame launch is the readiness boundary.
- [ ] Once the hold is lifted, inspect dirty state and the completed rebuild first. Integrate through Org source, tangle with the repo's existing command, evaluate the effective module and build before activation. Coordinate any daemon replacement with unsaved work. Test a separate frame/process rather than claiming a batch load proves GUI success.
- [ ] Run helper tests and Nix evaluation/VM tests, record command outputs and commit integration source separately from the user's unrelated changes.

## Task 8: Package, install and recover safely

**Files:** packaging/{build-manager.ps1,package.ps1,install.ps1,uninstall.ps1}, tests/acceptance/Test-Package.ps1, docs/{BUILD.md,OPERATIONS.md}.
**Interfaces:** package.ps1 -ServerStage <dir> -ManagerBinary <exe> -Output <dir> produces a versioned portable bundle plus manifest; install.ps1 -Bundle <dir> [-Root <dir>] [-EnableStartup]; uninstall.ps1 [-Root <dir>] preserves settings and removes only manifest-owned paths and matching startup entries. Default root: %LOCALAPPDATA%\Programs\VcXsrvWSL; state: %LOCALAPPDATA%\VcXsrvWSL.

- [ ] Test repeated installation preserves user config; paths with spaces work; missing/tampered artifacts reject before replacement; invalid/outside-root manifest paths reject; an unrelated file survives uninstall; an owned file replaced by the user is preserved/reported.
- [ ] Stage required server DLLs/fonts/xkb data and license notices from fresh build outputs using existing installer manifests as references. Derive product version from pinned source/build metadata, not the old hardcoded mkzip version. Do not bundle PuTTY/audio/SSH components for this product.
- [ ] Implement a portable per-user installer, shortcut and optional HKCU startup value. Verify stopped state before replacement, preserve the previous version for rollback, and never kill matching process names. Exact-root checks precede any recursive operation; no default purge of state/credentials.
- [ ] Include corresponding source/build information and a component license inventory. Clearly label unsigned development builds; do not claim signing or reproducible identical bytes unless measured.
- [ ] Run isolated install/uninstall/rollback tests outside the live install, then run the manager from the bundle. Commit packaging and operations documentation.

## Task 9: Live acceptance and final review

**Files:** docs/VALIDATION.md, tests/acceptance/Test-Server.ps1; fix only demonstrated failures in their owning components with regression tests.
**Interfaces:** Validation report contains test, exact build/config, command or reproduction, expected result, actual result, and PASS/FAIL/NOT RUN. No blanket compatibility/security claim replaces this report.

- [ ] Confirm the user lifted LIVE HOLD. Recheck Windows/WSL versions, the applied X11 Emacs build, real daemon, existing X servers and LeopardWM paths. Snapshot relevant configurations without collecting secrets.
- [ ] Launch the isolated bundle and complete real negative/positive X11 setup tests through direct VSOCK and the Unix relay. Verify no TCP listeners owned by the server; verify multiwindow and clipboard helpers actually work. Failure blocks activation of the managed profile.
- [ ] Open a separately identifiable X11 Emacs frame; verify LeopardWM enumeration, tiling, resize contents/input alignment, workspace switching, focus, dialogs, clipboard and scaling. Record executable/class information rather than assuming the absence of RAIL_WINDOW proves success.
- [ ] Coordinate restart/sleep tests: record work-loss impact, use the user's explicit go-ahead, then test both startup orders, new VM ID rebinding, reconnect after sleep and idle retirement. Stopping WSL terminates Emacs; verify a new daemon/frame, not impossible process preservation.
- [ ] Activate the per-user install and approved EmptyNet integration only after the tests above pass. Keep WSLg coexistence until an explicitly coordinated global cutover; document the reversal for both configurations.
- [ ] Run the required Rust/native/Nix/package checks once more only if fixes affected them. Request final independent review with the execution method the user selected, address findings, and produce the final report with artifact paths, source commits and remaining NOT RUN items.

## Self-review and execution handoff

Spec coverage: server/transport/authentication Tasks 1-3; manager/configuration/credentials Tasks 4-6; NixOS/Emacs Task 7; installer/build provenance Task 8; live behavior and rollback Tasks 8-9. The five Review Focus classes each have explicit checks in their owning tasks. Socket coexistence and internal-client transport are early validation gates, not assumed success.

Execution is underway using the user-approved subagent workflow. The user also approved the native Windows-local transport and lifted the guest testing hold. The task ledger records exact progress, revisions and remaining validation.

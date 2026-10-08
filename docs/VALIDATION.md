# VcXsrv WSL manager validation

Checkpoint: 2026-10-08. Source, build and isolated acceptance results are recorded below for their named revisions. Task9 is **not complete**: final bundle assembly/review, ordinary installation, activated-helper readiness and the normal Emacs daemon cutover remain pending. PASS does not mean the normal installed product has completed end-to-end acceptance.

## Tested source, binaries and host

| Item | Identity |
| --- | --- |
| Feature checkout | `C:/Users/walnu/source/vcxsrv-wsl-manager`, branch `feat/wsl-manager` |
| Current packaging correction | `7aefb20423f2984a2b7884e22ca69f277d19deed` |
| Historical packaging candidate, superseded | `d618a8aa7c2eaf77b6463a34b74af53d6bcafc5c`; preserved evidence, not final approved delivery |
| Server production source | `755ffc9aed2c12fe8d783c4f96caf9c2944342ae` |
| Server clean base build | `33f692c9babdfb69ec0785cdc6e234310f6b8921` |
| Guest module | `9fc0b54b021cc5d7c663433352d54e5d892680a8` |
| EmptyNet integration | `e59bf093ae296708da544dde7ee763b422da588e` |
| Current manager | 628,736 bytes; SHA256 `1CFBEB3A588C31B5F25AF5C7895B6E89D43B335E54201E28B88E023C5D5C01BD` |
| Server executable | 3,072,000 bytes; SHA256 `112298F1710F11FAF9B54481DC89297EDBCB282E83EFA8DE41CC019C30DA24EA` |
| Windows | Windows 11 Pro `10.0.26300.9457` |
| WSL / kernel / WSLg | `2.7.14.0` / `6.18.33.2-2` / `1.0.73.2` |
| LeopardWM runtime | `C:/Program Files/LeopardWM/bin/leopardwm.exe` |

The final server target was rebuilt incrementally in the completed clean-base workspace after the reviewed discovery quoting correction. Its build manifest preserves base/final revisions, source blob identities, patch hash and `dirty=true` provenance. This was **not a second clean-from-scratch final build** or a byte-reproducibility demonstration. Patch SHA256: `A90672C7496A02F4F1EB7073107AB649E047C5ADE7469B45B98D117B401818B7`. Evidence: `B/final-patched-server-stage/build-manifest.json`.

The native manager embeds an `asInvoker` manifest and uses Windows libraries plus the app-local MSVC runtime. It has no browser/WebView or Windows Python runtime. The previous 628,224-byte manager (`4B0FEB8EE20076B66CF73CD6995390A2DA2AEE51E629C0387FC257C5FC5D47F2`) belongs to historical runtime/package checks; it is not the current binary.

## Evidence locations and reproduction

The following abbreviations identify retained local evidence, not paths shipped to every recipient:

- `S/`: `C:/Users/walnu/source/vcxsrv-wsl-manager/.superpowers/sdd/2026-10-07-vcxsrv-wsl-manager/`.
- `B/`: `C:/Users/walnu/source/vcxsrv-build-tools/`. Earlier reports abbreviate this external directory as `tools/`; it is not this repository's `tools/`.
- Repository script paths below are relative to the feature checkout. Use PowerShell7 for `.ps1` commands. `<bundle>` means the exact hash-verified fixture identified by the result, not an arbitrary later bundle.

| ID | Reproduction command or procedure |
| --- | --- |
| R1 | `pwsh -NoProfile -File tests/native/all.ps1 -XcbBuildRoot <built-source-snapshot>`; recorded snapshot: `B/final-wrapper-stage.work-d97cde35-accb-4d75-8f58-95052d7fadb0`. See [native test requirements](../tests/native/README.md). |
| R2 | `cargo test --manifest-path manager/Cargo.toml --locked`; `cargo clippy --manifest-path manager/Cargo.toml --locked --all-targets -- -D warnings`; `pwsh -NoProfile -File packaging/build-manager.ps1`. |
| R3 | `tests/acceptance/Test-Server.ps1` accepts explicit `-Server`, `-Authority`, `-ServerProcessId`, `-UnixEndpoint` and guest arguments. `x11-probe.ps1` / `x11-probe.py` exercise absent, wrong and valid cookie setup. Use only a separately owned test server; keep authority bytes out of command lines and logs. Recorded live procedures/results: `S/live-acceptance-evidence.md`, `B/manager-real-fixed.log`, `B/manager-real-reuse.log`, `B/manager-bundle.log`. |
| R4 | On Linux: `python3 -m unittest discover -s integration/nixos -v`. Consumer-pinned Nix evaluation/VM expression and formatting commands: [guest README](../integration/nixos/README.md), `S/task-7-report.md` and `S/task-7-fixround1-report.md`. |
| R5 | In `/home/Empty/src/EmptyNet`, inherit `NIX_CONFIG='experimental-features = nix-command flakes'`: `nix develop --command just tangle`, stage intended outputs, `nix develop --command just tangle-check`, `nix develop --command just eval`, then `nixos-rebuild build --flake .#mcqueenix`. Build only; activation is separate. |
| R6 | `pwsh -NoProfile -File tests/acceptance/Test-InstallGuard.ps1` and `tests/acceptance/Test-GuardLifetime.ps1`. |
| R7 | `pwsh -NoProfile -File tests/acceptance/Test-PackageReview.ps1 -Bundle <bundle> -Case <case>`; cases `first`, `edit`, `uninstall`, `copy`, `ledger`, `replacement`, `appearance`, `identity`. Also run `Test-PackageMatrix.ps1 -Bundle <bundle>` and `Test-PackageStateBinding.ps1 -Bundle <bundle>`. Recorded fixture versions are in `S/task-8-package-fix-report.md`. |
| R8 | `pwsh -NoProfile -File tests/acceptance/Test-Package.ps1 -Bundle <bundle>`; full isolated install/reinstall/rollback/uninstall fixture. |
| R9 | Actual desktop entry procedure: `B/test-manager-first-launch.ps1` via Explorer/PowerShell7, separately executing the generated shortcut and exact generated Run command with absent isolated state and deliberately missing companion. Rehearsal of the exact built login shell: `B/verify-emacs-login-environment.py`. These retained controller procedures are not default automated-suite tests. |
| R10 | `packaging/package.ps1` assembles the delivery; inspect `bundle-manifest.json`, ZIP CRC/source inventory, committed source correspondence and locked offline Cargo metadata. Build/source reconstruction requirements are in [BUILD.md](BUILD.md). Final refreshed assembly is pending this documentation checkpoint. |

## Recorded results

PASS means the stated expected behavior was observed for the named source/fixture. NOT RUN includes pending work and checks not established by another layer's evidence. Earlier FAIL observations remain retained; they are not rewritten as successful runs.

| Check / reproduction | Expected | Actual and evidence | Status |
| --- | --- | --- | --- |
| Server native suite / R1, source `755ffc9` | Transport/auth/admission/discovery regressions pass and affected production units compile | Full suite exit0; `B/final-native-suite.log`. Final full-target link recorded in server build manifest. Legacy C conversion/sign and clipboard declaration warnings remain. | PASS |
| Current manager / R2, `7aefb204` | Default tests pass, strict lint and release build succeed | 55 passed, 0 failed, six explicit ignored; `S/task8-review-fix-native.log`, `task8-review-fix-clippy.log`, `task8-review-fix-build.log`. | PASS |
| Desktop manager lifecycle / R3, historical resource-path correction | Exact server identity survives ordinary exit/restart; resources resolve from unrelated CWD | Explorer lifecycle suite 15/15; `B/manager-resource-green.log`. Actual initialization/detach/reuse in `B/manager-real-fixed.log` and `manager-real-reuse.log`; authority privately compared unchanged. Later installation-guard/startup changes have separate tests below. | PASS, historical scope |
| Abrupt manager fixture / desktop lifecycle | Process exit without Rust destructors leaves recoverable server fixture | Passed explicit fixture. Not an injected access violation or proof against external job/session teardown. | PASS, bounded fixture |
| Stopped-record recovery / R3 | Retire only positively stopped metadata; preserve authority | Actual `recover-stopped` succeeded; unknown/live identities refused by tests. `S/live-acceptance-evidence.md`. | PASS |
| Real bundled resource initialization / R3, historical preview | Keyboard, GL and authenticated clipboard initialize outside source CWD | `B/manager-bundle.log`, `.exit` and `manager-bundle-result.json`; display3 server initialized and survived tray exit. Expected missing guest helper kept Ready false. | PASS, historical preview |
| Local AF_UNIX and direct VSOCK / R3, final server `755ffc9` | Absent/wrong cookies rejected; correct cookie accepted | All initialized probes matched expected outcomes; `S/live-acceptance-evidence.md`, `B/manager-real-fixed.log`, `B/manager-bundle.log`. | PASS |
| Server TCP exposure / R3 | No server-owned TCP listening socket | Explicit listening-socket inspection found none for tested real server identities. Same live logs. | PASS |
| Private guest socket and installed Xlib / R3 | Selector0 authority authenticates over private Unix socket; WSLg mount untouched | Separate X11 Emacs30.2 test process connected; temporary test relay/socket and authority had private permissions. `S/live-acceptance-evidence.md`. Not proof of activated declarative integration. | PASS, isolated live process |
| LeopardWM admission / live procedure in R3 | First real native SHOW has APPWINDOW; ordinary frame admitted without refresh | Task2c tracing and final `755ffc9` Emacs window automatically admitted. `S/live-acceptance-evidence.md`; retained-HWND state regression covered by R1. | PASS |
| Resize, focus, repeated mapping / live procedure in R3 | Frame geometry/focus follow tiler; remapped frames admitted | Width 1117→1237→1117, focus nil/t, five hide/remap cycles. These recorded GUI checks span pre-Task2c/Task2c; not a fresh final-package GUI suite. | PASS, named historical GUI checks |
| Rendering, typing and pointer alignment / user observation | Separate test windows render and respond correctly | User explicitly confirmed both windows. Automated visual helper unavailable; current monitor configuration only. `S/live-acceptance-evidence.md`. | PASS, user-observed |
| Clipboard / live procedure in R3 | Both directions transfer the expected marker; original clipboard restored | Linux→Windows and Windows→actual Emacs selection passed at Task2c. Bundled final-server authenticated clipboard initialization also passed. | PASS |
| Guest helper / R4, `9fc0b54b` | Record/path/lock/exec and bounded startup retries satisfy contract | 31 tests passed; `S/task7-fixround1-helper.log`. Real Unix drop→setup→exec, full10s deadline, explicit rejection/malformed/truncated-header failures covered. | PASS |
| Nix module and isolated VM / R4 | Effective module asserts correctly; real service/socket/auth lifecycle works | 17 evaluation checks; real KVM systemd/socat cleanup/restarts/conflicts and Xvfb positive/negative auth plus Xlib selector lookup passed. `S/task7-eval-final.log`, `task7-vm-final.log`. VM predates mechanical formatting and later transport-retry fix; focused tests cover that fix, no subsequent VM rerun claimed. | PASS, recorded revision boundary |
| EmptyNet / R5, `e59bf093` | Eight exact vendor files, Org/generated agreement, evaluable and buildable system | Provenance, effective config and closure build passed; `S/task7-vendor-provenance.json`, `task7-emptynet-effective.json`, `task7-emptynet-build.log`, `task7-emptynet-final-proof.json`. Nix reported staged dirty-tree/custom-output and skipped incompatible aarch64-darwin warnings; Linux gate passed. | PASS |
| Exact built login shell / R9 | Login profile preserves DISPLAY/XAUTHORITY and removes WAYLAND_DISPLAY | `B/emacs-login-environment.json`: intended private paths retained, WAYLAND_DISPLAY null, matches=true, daemonStarted=false. Used exact built unit shell; not an Emacs daemon start. | PASS, rehearsal only |
| Installation guard / R6 | Retain singleton/stopped proof and exact-process ownership across handoff | `S/task8-guard-fix-protocol.log`, `task8-guard-lifetime.log`: newline RELEASE without EOF, crash/timeout, parent exit before/after duplication and living-child noninheritance passed. | PASS |
| Corrected package ownership/failures / R7, `7aefb204` | Refuse late edits/new unowned paths/changed ledger identity; restore failed uninstall; clean only created partial files | `S/task8-review-green-{edit,uninstall,copy,ledger,replacement,appearance,identity}.log`; complete matrix/state results in `task8-review-fix-matrix.log`, `task8-review-fix-state.log`. | PASS |
| Startup/shortcut ownership / R7 | Startup off by default, opt-in isolated, externally changed entries preserved | Matrix and `S/task8-review-green-first.log` passed. No normal startup registration activated. | PASS |
| Actual installed-entry cold launch / R9, current manager hash above | Generated shortcut and Run command initialize absent state without explicit missing-config failure | Both created protected config, selected fixture companion path, left authority/ownership absent and exited normally with exit0. `B/installed-first-launch.log`, `.exit`, `installed-first-launch-results.json`. Deliberately absent companion prevented any server/guest startup. | PASS, first-launch configuration only |
| Full bundle maintenance / R8, historical `task8-bundle-884fa90-r1` | Install/reinstall/rollback/uninstall preserve modified/unrelated/state files | `S/task8-package-actual.log` passed in a spaced isolated root. Its installer predates later review corrections; this does not establish current full-size fixture success. | PASS, historical preview |
| Corrected full-size bundle maintenance / R8 | Repeat meaningful full runtime fixture after copy/uninstall changes | `S/task8-review-fix-actual.log`, fixture `B/task8-fix1-preview-dirty`: PASS exit0 for first/repeat installation, rollback and transactional uninstall. Only modified protocol.txt and unrelated.txt remain; ledger/quarantine payload absent. Source delivery was explicitly pending in this internal fixture; final manifest correspondence binds tested runtime/scripts to delivery. | PASS, corrected full-size fixture |
| Historical corresponding source / R10, `d618a8aa7` | ZIP integrity, exact source correspondence and all13 locked crates resolve offline | `S/task8-final-artifact-proof.json` and `task8-source-offline-metadata.json` record success for the superseded candidate. Not evidence of refreshed post-correction delivery. | PASS, historical candidate |
| Final refreshed artifact assembly / R10 | Immutable manifest matches approved source, current manager and retained server/runtime bytes | Waits for this documentation commit and final assembly/review; final identities must come from that bundle and external receipt. | NOT RUN / pending |

The six exclusions in the default manager suite are explicit: `inspect_fixture_parent_job`, `manager_normal_and_abrupt_exit_preserve_recoverable_fixture`, `persistent_handle_detaches_recovers_and_stops_only_valid_identity`, `persistent_job_report`, `persistent_server_loads_module_relative_mixed_paths_and_installation_cwd`, and `installed_wsl_running_list`. Desktop checks and manual job diagnostics are separate from the 55 default passes; no ignored test is counted as passed.

Retained FAIL evidence includes discovery quoting, admission ordering/retained-HWND state, resource paths, tray reentrancy, guest startup retries, guard framing/association and installer rollback/state binding. The current package correction specifically follows RED failures for missing-config first launch, post-preflight edits, partial uninstall and partial copy/ledger cleanup. See `S/task-8-package-fix-report.md` for exact RED/GREEN logs. Corrected passes do not erase the earlier attempts.

## Artifact identity and historical previews

The source document precedes final assembly. The delivered immutable `bundle-manifest.json` records the actual source revision, manager/server provenance, file sizes and SHA256 values, including the corresponding-source archive. The controller's separate artifact receipt records that manifest's own hash after assembly. A document inside the archive cannot embed the hash of the final archive containing itself; this checkpoint intentionally leaves that identity to the manifest and external receipt.

Historical preview `B/task8-bundle-884fa90-r1` had manifest SHA256 `54F7F530D6C1258D15B7B2C6BFC29CACF683609CE4D06CB87AD9F915D47D1530`, 5,183 payload files /202,495,934 bytes, including a119,135,675-byte source archive. It was explicitly provisional/dirty. Its separate display3 server was stopped through a retained handle after path/full creation-time verification, and explicit recovery preserved authority. The normal daemon was not stopped.

`B/task8-final-bundle-d618a8aa7` is also historical despite its directory name: package review found defects subsequently corrected at `7aefb204`. `B/task8-fix1-preview-dirty` is an internal corrected-runtime fixture with pending source delivery, not the final approved bundle. Compare final runtime hashes before carrying forward previous acceptance; never infer final delivery from a directory named “final.”

## Built guest configuration and remaining activation

| Item | Recorded exact value |
| --- | --- |
| Built system | `/nix/store/5bjissnca8dwyyjki4qf3ks2yzbjb7b2-nixos-system-mcqueenix-26.05.20260917.ecc58f3` |
| Built helper | `/nix/store/5dfz86klglcyda27wm3bpj31x1w12kiv-vcxsrv-wsl/bin/vcxsrv-wsl` |
| Built X11 Emacs | `/nix/store/d18fj8z2i7bnh83w5vmhyikrrdk8dnz4-emacs-30.2` |
| Built login-shell unit | `/nix/store/n6jn4wxkphvab0505bfqzgn33fi1rd9h-emacs.service/emacs.service` |
| Rehearsed shell | `/nix/store/axl12yr00f105bjqvj628f5f0s6r0cnp-bash-5.3p9/bin/bash -l -c` |
| Intended daemon environment | `DISPLAY=unix:/run/user/1000/vcxsrv-wsl/X1`; `XAUTHORITY=/run/user/1000/vcxsrv-wsl/authority`; WAYLAND_DISPLAY absent. UID1000 is this machine's account, not a hardcoded module UID. |
| Preserved active system at post-build check | `/nix/store/bdpc6q931n62ghm3mlbpg8p20wmgxi6k-nixos-system-mcqueenix-26.05.20260917.ecc58f3` |
| Preserved normal daemon | `emacs.service`, PID8671; `/nix/store/1idk29gczsy4myysz1ac4rh5n1nhjrzr-emacs-pgtk-30.2/bin/.emacs-30.2-wrapped` |

The daemon has Wants/After on the relay and `X-RestartIfChanged=false`. Effective source/unit selection and a login-shell diagnostic do not prove that the preserved daemon changed backend. Earlier GUI acceptance used separate X11 processes.

| Remaining acceptance | Expected evidence | Status |
| --- | --- | --- |
| Ordinary per-user manager/server installation plus real configured helper | Activated exact closure, installed helper path, authenticated manager Ready and Launch Emacs through the private relay | NOT RUN |
| Normal daemon replacement | Coordinated replacement after saving work; exact new executable, actual DISPLAY/XAUTHORITY/Wayland environment and a real frame | NOT RUN |
| WSL restart/startup orders and VM-ID rebinding | Coordinated shutdown/restart with observed safe rebinding/recovery | NOT RUN |
| Sleep/resume and idle retirement | Actual lifecycle behavior with relay and existing linger; host-only polling is insufficient evidence | NOT RUN |
| Mixed DPI/scaling and additional dialog/menu/workspace switching | Separate visual/input observations beyond existing resize/focus/remap checks | NOT RUN |
| Second-account isolation and broader crash/session/job teardown cases | Explicit token/lifecycle experiments for each claimed boundary | NOT RUN |
| Global WSLg disable/cutover | Separately approved change and validation; unnecessary for initial coexistence | NOT RUN |

No normal daemon restart, guest activation, WSL shutdown or sleep test occurred in this checkpoint. [OPERATIONS.md](OPERATIONS.md) describes the coordinated activation/reversal procedure. A persistent relay and linger may affect WSL idle retirement; that remains an experiment, not a guarantee.

## Known nonblocking follow-ups

The final whole-branch source review through `718a352ef` found no Critical or Important findings. Its two Minor findings were explicitly retained by the controller:

- **M1, console help:** `xorg-server/hw/xwin/InitOutput.c:760` still says distro discovery accepts only ASCII names. Runtime uses the original wide Windows arguments and supports validated Unicode/spaced names; tests cover those boundaries. The sentence is a documentation defect scheduled for the next source/binary refresh. The tested server binary is preserved.
- **M2, maintenance time:** the corrected full-size fixture ran from 03:27:56 to 03:55:59 local time on 2026-10-08, about **28m03s** total. Initial install completed within about 3m31s, repeat by 9m05s cumulative, rollback took about 8m07s, and uninstall about 10m51s. These approximate phase observations include validation/staging/controller overhead and two retained rollback copies, not a benchmark. Evidence: `S/task8-review-fix-timing.json` and `task8-review-fix-actual.log`. Per-file old-entry scans and repeated ledger hashing merit future profiling/indexed lookup with immediate ownership validation preserved; no performance tuning or validation removal was made in this artifact.

## Operating limits

The installer coordinates trusted same-user operations; it is not a security boundary against malicious same-user code or arbitrary uncoordinated launches. VM binding is not per-distro isolation. Ordinary manager exit detaches, but an external supervisor's job teardown or Windows session termination may still end the server. Storage recovery can itself fail; preserved receipts identify remaining owned state.

This is an unsigned local development build. No public release, signing, blanket security equivalence to WSLg, or byte-identical reproducibility is claimed. Detailed local evidence remains under `S/` and `B/`; evidence retention does not imply all scratch logs are distributed. [BUILD.md](BUILD.md) documents reconstruction and [READING-ORDER.md](READING-ORDER.md) maps the source.

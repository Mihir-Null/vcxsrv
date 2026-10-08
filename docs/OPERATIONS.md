# VcXsrv WSL Manager operations

This is an unsigned local development build. Package hashes detect accidental changes; they do not authenticate a public publisher. Source, patch, component notices and locked Rust dependency source are delivered with the bundle.

## Windows installation

Use PowerShell7. Stop the owned server explicitly in the manager, then close the manager before maintenance. Closing the tray alone leaves the server running. Install from a bundle outside the destination:

```powershell
pwsh -NoProfile -File <bundle>/tools/install.ps1 -Bundle <bundle>
```

Default installation is `%LOCALAPPDATA%/Programs/VcXsrvWSL`; protected state/config/authority are `%LOCALAPPDATA%/VcXsrvWSL`. A per-user Start menu shortcut starts the native tray. Startup is off by default; pass `-EnableStartup` to opt in. No elevation, service, firewall or global PATH edits are performed. The native fixed guard holds the existing SID singleton through replacement; live/unknown ownership and running install-associated processes refuse maintenance. Never stop processes by name.

Keep the installation root stable: manager-generated config stores an absolute companion path. Existing maintenance inherits the recorded state profile when omitted and refuses a different supplied state path; checking another profile cannot authorize replacement. Installation preserves config and authority. Existing modified owned files cause upgrade refusal; inspect the named files before retrying. Unrelated files stay in place. The previous matching owned payload and ledger are retained under `.rollback/<id>`.

```powershell
pwsh -NoProfile -File <bundle>/tools/install.ps1 -Root <installation> -Rollback
pwsh -NoProfile -File <bundle>/tools/uninstall.ps1 -Root <installation>
```

Rollback also requires stopped state. Uninstall removes only manifest-owned files still matching their hashes and matching shortcut/startup entries; user-modified or unrelated files and private state remain. It leaves empty directories rather than recursively deleting an uncertain root. Run an uninstall script from the bundle or another location outside the installation being removed.

Uninstall first moves matching owned files into `.uninstall/<id>` under the installation and retains a recovery receipt. A late locked file restores earlier moves before releasing the guard; shortcut/startup removal follows the payload moves and is also restored on failure. If storage itself prevents recovery, the receipt and payload remain at the reported path for diagnosis. Cleanup failure after the logical uninstall preserves the receipt rather than recursively deleting an uncertain tree. Installer temporary writes retain exclusive write ownership and clean their exact created file handles; a replacement at the temporary pathname survives. Each payload/ledger replacement reinspects current ownership and root safety immediately after staging.

If a valid stopped record remains after maintenance, run the installed manager's explicit `recover-stopped` before Start. This retires only positively stopped metadata and preserves authority. Invalid/unknown records require diagnosis; reinstall is not credential recovery. `doctor --json` is read-only and never establishes live authenticated readiness.

## Guest activation and reversal

Task7 core helper/module source is vendored into the authoritative EmptyNet Org source, commit `e59bf093ae296708da544dde7ee763b422da588e`. Evaluated/build-only mcqueenix closure: `/nix/store/5bjissnca8dwyyjki4qf3ks2yzbjb7b2-nixos-system-mcqueenix-26.05.20260917.ecc58f3`. Helper: `/nix/store/5dfz86klglcyda27wm3bpj31x1w12kiv`; X11 Emacs build identified by controller as d18f. Build success is not activation: normal daemon PID8671 still uses the old pgtk generation.

Before guest activation, save Emacs work and coordinate replacement of the daemon: switching its backend cannot preserve its existing process. The controller must record the current system generation/profile and service state, activate the reviewed exact closure through the repository's normal deployment flow, then restart the configured relay/daemon only under the user's approved maintenance window. Confirm actual `vcxsrv-wsl` path, target-user UID-derived private runtime socket/authority permissions, X11 daemon environment, and a separately identifiable real frame through the installed manager. WSLg's mount stays intact.

The verified normal deployment entry point and units are below. These are pending activation instructions, not completed steps. Run as guest user Empty (UID1000) after the coordinated approval and after recording the prior system generation and `/run/current-system` target:

```sh
sudo nixos-rebuild switch --flake /home/Empty/src/EmptyNet#mcqueenix
systemctl --user status vcxsrv-wsl-relay.service emacs.service
systemctl --user restart emacs.service
```

The switch runs `home-manager-Empty.service`; `emacs.service` deliberately has `X-RestartIfChanged=false`, so switching alone preserves the old daemon. Compare the resulting system with the reviewed closure above, verify `/proc/<new-daemon-pid>/exe` and the actual X11 daemon environment, then launch a real frame through the manager. Scoped launcher variables alone are insufficient. Disabling WSLg or shutting down WSL requires a separate coordinated step and is unnecessary for the initial coexistence test.

To reverse, select the recorded prior NixOS generation (`nixos-rebuild switch --rollback` only when it is that recorded generation), restart the prior daemon in a coordinated window, and verify a frame in its previous display backend. Revert the named EmptyNet integration commit through normal source review if retiring the integration; do not discard unrelated user changes. Windows rollback/uninstall is independent and preserves credentials/state.

## Lifetime and trust boundaries

Ordinary manager exit or crash detaches from its server. Windows sign-out/shutdown or an external launcher tearing down its process group can stop it. Start from the ordinary installed shortcut; restricted launchers that forbid breakaway produce an actionable error. Modern Standby job membership alone does not identify a defect or prove survival of external policy.

The protected Windows SID owns manager state and process identity. Guest bootstrap binds a configured guest user/UID, distro and VM discovery; VSOCK host communication is authenticated by the X cookie. This is not per-distro isolation or a blanket hardening/WSLg-equivalence claim. Secrets never appear in argv or logs. Do not change access-control policy to bypass failed authentication.

## Acceptance boundary

Controller evidence records source-built final-server native suite PASS; missing/wrong cookies rejected and valid cookies accepted locally and through direct VSOCK; no owned TCP listeners; native Emacs admission, clipboard, detach and reuse PASS. Rendering/input alignment PASS was explicitly user-observed. Current Task6 real server reuse/init and Task7 unit/evaluation/build checks passed; guest final activation and real installed-product end-to-end acceptance remain pending.

Task8 isolated package/install/rollback evidence is recorded in its report. The preview bundle launched the actual server from an unrelated working directory with GL/keyboard initialization, authenticated clipboard, detach and no TCP listeners; direct VSOCK rejected missing/wrong cookies and accepted the correct cookie. The expected missing guest helper kept readiness false. Parent-exit and living-child guard lifetime checks passed. Final refreshed artifacts must match the tested manager/server/data hashes; these checks do not establish installed guest end-to-end readiness. Sleep/resume, WSL restart, idle retirement and mixed DPI/scaling remain NOT RUN for the final activated product. Current host baseline is recorded in `tools/final-host-baseline.json` by the controller (Windows11Pro10.0.26300.9457, WSL2.7.14.0, kernel6.18.33.2-2, WSLg1.0.73.2).

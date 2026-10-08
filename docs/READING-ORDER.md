# Editable source reading order

1. `docs/superpowers/specs/2026-10-07-vcxsrv-wsl-manager-design.md`: intended behavior and explicit ownership/security boundaries.
2. `manager/src/config.rs`, `cli.rs`, `process.rs`, `storage.rs`: versioned configuration, literal command arguments, disposable-helper containment and protected SID-owned state. Helpers have kill-on-close jobs; persistent server ownership is separate.
3. `manager/src/instance.rs`, `owner.rs`, `recovery.rs`, `controller.rs`, `worker.rs`: singleton handle presence, exact PID/creation/file identity, explicit stopped retirement, state/effect lifetime and cancellation.
4. `manager/src/authority.rs`, `tray.rs`, `startup.rs`: binary Xauthority, native UI, confirmed Stop and optional per-user startup.
5. `integration/nixos/README.md`, `helper.py`, `default.nix`, `home-manager.nix`: guest UID-derived runtime paths, private relay, bounded authenticated readiness and X11 Emacs launcher. Authoritative personal deployment source lives in EmptyNet Org, not a hidden editor-only override.
6. `xorg-server/hw/xwin/winwslprofile.c`, `winwsldiscovery.c`, and adjacent transport/readiness code: managed authority and real ClientAuthorized gate, bounded VM discovery, Windows-local transport and initialized readiness.
7. `xorg-server/hw/xwin/winmultiwindowwindow.c` and native admission tests: initial hidden preparation and first visible mapping so LeopardWM sees normal windows without refresh.
8. `packaging/build-manager.ps1`, `build-server.ps1`, `package.ps1`, `Package.psm1`, `install.ps1`, `uninstall.ps1`, and `manager/src/installation.rs`: retained-build provenance, full runtime/notices/source inventory, fixed guarded maintenance, matching-file preservation and rollback. Read `docs/BUILD.md` and `OPERATIONS.md` beside these.

The server build archive preserves actual retained source/generator substitutions and base33f692 → reviewed755ffc9 patch provenance. It is an incremental final full-target build; source pinning does not imply identical reproducible bytes. Keep automated checks distinct from actual user-observed GUI and final activated integration acceptance.

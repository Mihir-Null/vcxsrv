# One target-user relay; immutable helper config contains no credentials.
{
  config,
  lib,
  pkgs,
  options,
  ...
}:
let
  cfg = config.services.vcxsrvWsl;
  hasHomeManager = options ? home-manager;
  targetHome = if hasHomeManager then config.home-manager.users.${cfg.user} or null else null;
  helperConfig = pkgs.writeText "vcxsrv-wsl-config.json" (
    builtins.toJSON {
      inherit (cfg) user display port;
      socat = "${pkgs.socat}/bin/socat";
      systemctl = "${pkgs.systemd}/bin/systemctl";
      emacsclient = "${pkgs.emacs}/bin/emacsclient";
    }
  );
  helper = pkgs.runCommand "vcxsrv-wsl" { meta.mainProgram = "vcxsrv-wsl"; } ''
    mkdir -p "$out/bin" "$out/etc"
    cp ${./helper.py} "$out/bin/vcxsrv-wsl"
    substituteInPlace "$out/bin/vcxsrv-wsl" \
      --replace-fail '#!/usr/bin/env python3' '#!${pkgs.python3}/bin/python3'
    chmod 0555 "$out/bin/vcxsrv-wsl"
    cp ${helperConfig} "$out/etc/config.json"
  '';
in
{
  options.services.vcxsrvWsl = {
    enable = lib.mkEnableOption "private authenticated X11 relay to managed VcXsrv over VSOCK";
    user = lib.mkOption {
      type = lib.types.str;
      default = "";
      description = "Existing target account; its runtime UID is resolved by the helper.";
    };
    display = lib.mkOption {
      type = lib.types.int;
      default = 1;
      description = "Server display number, in 0..99. The client pathname selector is always 0.";
    };
    port = lib.mkOption {
      type = lib.types.int;
      default = 106000 + cfg.display;
      description = "VSOCK port; must equal 106000 + display.";
    };
    package = lib.mkOption {
      type = lib.types.package;
      readOnly = true;
      default = helper;
      description = "Fixed guest helper with embedded nonsecret declarative configuration.";
    };
  };
  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      {
        assertions = [
          {
            assertion =
              cfg.user != ""
              && builtins.hasAttr cfg.user config.users.users
              && config.users.users.${cfg.user}.isNormalUser;
            message = "vcxsrv-wsl: an existing normal target account is required.";
          }
          {
            assertion = cfg.display >= 0 && cfg.display <= 99;
            message = "vcxsrv-wsl: display must be in 0..99.";
          }
          {
            assertion = cfg.port == 106000 + cfg.display;
            message = "vcxsrv-wsl: port must equal 106000 + display.";
          }
          {
            assertion = hasHomeManager;
            message = "vcxsrv-wsl: import the Home Manager NixOS module.";
          }
          {
            assertion = targetHome != null && targetHome.vcxsrvWsl.enable;
            message = "vcxsrv-wsl: enable vcxsrvWsl in the target Home Manager account.";
          }
          {
            assertion = targetHome == null || targetHome.vcxsrvWsl.display == cfg.display;
            message = "vcxsrv-wsl: system and target home display numbers must match.";
          }
        ];
        environment.systemPackages = [ helper ];
        systemd.user.services.vcxsrv-wsl-relay = {
          description = "Private X11 Unix socket to managed VcXsrv VSOCK";
          wantedBy = [ "default.target" ];
          unitConfig = {
            ConditionUser = cfg.user;
            StartLimitIntervalSec = 30;
            StartLimitBurst = 3;
          };
          serviceConfig = {
            ExecStart = "${helper}/bin/vcxsrv-wsl _relay";
            Type = "simple";
            Restart = "on-failure";
            RestartSec = 1;
            TimeoutStopSec = 6;
            KillMode = "control-group";
            UMask = "0077";
            NoNewPrivileges = true;
            RestrictAddressFamilies = [
              "AF_UNIX"
              "AF_VSOCK"
            ];
            TasksMax = 128;
            # Shared authority outlives service restarts. The helper validates
            # and creates its directory; RuntimeDirectory would repair unsafe
            # paths before that validation and could remove live credentials.
          };
        };
      }
      (lib.optionalAttrs hasHomeManager {
        home-manager.sharedModules = [ ./home-manager.nix ];
      })
    ]
  );
}

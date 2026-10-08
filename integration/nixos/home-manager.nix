# Extend the existing kittymacs/HM daemon, without emitting another relay.
{
  config,
  lib,
  pkgs,
  osConfig ? { },
  options,
  ...
}:
let
  cfg = config.vcxsrvWsl;
  system =
    osConfig.services.vcxsrvWsl or {
      enable = false;
      user = "";
      display = 1;
    };
  hasKittymacs = options.programs ? kittymacs;
in
{
  options.vcxsrvWsl = {
    enable = lib.mkEnableOption "kittymacs frames on the managed VcXsrv relay";
    display = lib.mkOption {
      type = lib.types.int;
      default = system.display;
      description = "Must match the NixOS relay display number.";
    };
  };
  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      {
        assertions = [
          {
            assertion = system.enable && system.user == config.home.username;
            message = "vcxsrv-wsl: the enabled system relay must target this home account.";
          }
          {
            assertion = cfg.display >= 0 && cfg.display <= 99 && cfg.display == system.display;
            message = "vcxsrv-wsl: home display must match the enabled system relay in 0..99.";
          }
          {
            assertion = hasKittymacs && config.programs.kittymacs.enable && config.programs.kittymacs.daemon;
            message = "vcxsrv-wsl: import/enable kittymacs with its existing daemon.";
          }
          {
            assertion = config.services.emacs.enable;
            message = "vcxsrv-wsl: the real Home Manager Emacs service must be enabled.";
          }
        ];
        systemd.user.services.emacs = {
          Unit = {
            Wants = [ "vcxsrv-wsl-relay.service" ];
            After = [ "vcxsrv-wsl-relay.service" ];
            X-RestartIfChanged = false;
          };
          Service = {
            Environment = lib.mkAfter [
              "DISPLAY=unix:%t/vcxsrv-wsl/X${toString cfg.display}"
              "XAUTHORITY=%t/vcxsrv-wsl/authority"
            ];
            UnsetEnvironment = [ "WAYLAND_DISPLAY" ];
          };
        };
      }
      (lib.optionalAttrs hasKittymacs { programs.kittymacs.package = pkgs.emacs; })
    ]
  );
}

# Evaluate against the exact pinned input set supplied by the consumer.
{
  pkgs,
  homeManager,
  kittymacs,
}:
let
  inherit (pkgs) lib;
  base = {
    imports = [
      homeManager.nixosModules.home-manager
      ./default.nix
    ];
    users.users.Empty = {
      isNormalUser = true;
      uid = 1000;
      linger = true;
    };
    services.vcxsrvWsl = {
      enable = true;
      user = "Empty";
    };
    home-manager = {
      useGlobalPkgs = true;
      useUserPackages = true;
      users.Empty = {
        imports = [ kittymacs.homeManagerModules.default ];
        home.stateVersion = "26.05";
        programs.kittymacs = {
          enable = true;
          daemon = true;
          installFonts = false;
          installPackages = false;
        };
        vcxsrvWsl.enable = true;
        services.emacs.startWithUserSession = false;
      };
    };
    system.stateVersion = "26.05";
    boot.loader.grub.enable = false;
    fileSystems."/" = {
      device = "tmpfs";
      fsType = "tmpfs";
    };
  };
  evaluate =
    modules:
    (import (pkgs.path + "/nixos/lib/eval-config.nix") {
      system = pkgs.stdenv.hostPlatform.system;
      inherit modules;
    }).config;
  valid = evaluate [ base ];
  rejects = modules: builtins.any (entry: !entry.assertion) (evaluate modules).assertions;
  home = valid.home-manager.users.Empty;
  checks = {
    validAssertions = builtins.all (entry: entry.assertion) valid.assertions;
    oneRelay =
      builtins.attrNames (
        lib.filterAttrs (name: _: lib.hasPrefix "vcxsrv-wsl" name) valid.systemd.user.services
      ) == [ "vcxsrv-wsl-relay" ];
    targetUser = valid.systemd.user.services.vcxsrv-wsl-relay.unitConfig.ConditionUser == "Empty";
    consistentPort = valid.services.vcxsrvWsl.port == 106001;
    x11Package = home.programs.kittymacs.package.drvPath == pkgs.emacs.drvPath;
    preservedDaemon = home.systemd.user.services.emacs.Unit.X-RestartIfChanged == false;
    orderedDaemon = builtins.elem "vcxsrv-wsl-relay.service" home.systemd.user.services.emacs.Unit.After;
    wantedDaemon = builtins.elem "vcxsrv-wsl-relay.service" home.systemd.user.services.emacs.Unit.Wants;
    noKillDependency =
      !(home.systemd.user.services.emacs.Unit ? BindsTo)
      && !(home.systemd.user.services.emacs.Unit ? Requires);
    explicitEnvironment =
      builtins.elem "DISPLAY=unix:%t/vcxsrv-wsl/X1" home.systemd.user.services.emacs.Service.Environment
      && builtins.elem "XAUTHORITY=%t/vcxsrv-wsl/authority" home.systemd.user.services.emacs.Service.Environment;
    missingUser = rejects [
      base
      { services.vcxsrvWsl.user = lib.mkForce "missing"; }
    ];
    missingHome = rejects [
      base
      { home-manager.users = lib.mkForce { }; }
    ];
    disabledHome = rejects [
      base
      { home-manager.users.Empty.vcxsrvWsl.enable = lib.mkForce false; }
    ];
    mismatchedDisplay = rejects [
      base
      { home-manager.users.Empty.vcxsrvWsl.display = 2; }
    ];
    inconsistentPort = rejects [
      base
      { services.vcxsrvWsl.port = 106000; }
    ];
    invalidDisplay = rejects [
      base
      { services.vcxsrvWsl.display = 100; }
    ];
    missingHmImport = rejects [
      {
        imports = [ ./default.nix ];
        services.vcxsrvWsl = {
          enable = true;
          user = "Empty";
        };
        users.users.Empty.isNormalUser = true;
        system.stateVersion = "26.05";
        boot.loader.grub.enable = false;
        fileSystems."/" = {
          device = "tmpfs";
          fsType = "tmpfs";
        };
      }
    ];
  };
  successful = builtins.all (value: value) (builtins.attrValues checks);
in
{
  evaluation =
    assert successful;
    pkgs.writeText "vcxsrv-wsl-module-evaluation.json" (builtins.toJSON checks);
  vm = pkgs.testers.runNixOSTest {
    name = "vcxsrv-wsl-isolated";
    nodes.machine = { ... }: {
      imports = [ base ];
      environment = {
        systemPackages = [
          pkgs.python3
          pkgs.xorg-server
          pkgs.socat
          pkgs.iproute2
        ];
        etc = {
          "vcxsrv-wsl-tests/vm_test.py".source = ./vm_test.py;
          "vcxsrv-wsl-tests/helper.py".source = ./helper.py;
          "vcxsrv-wsl-tests/libX11".text = "${pkgs.libx11}/lib/libX11.so.6";
        };
      };
      virtualisation.memorySize = 2048;
    };
    testScript = ''
      start_all()
      machine.wait_for_unit("user@1000.service")
      machine.succeed("runuser -u Empty -- env XDG_RUNTIME_DIR=/run/user/1000 python3 /etc/vcxsrv-wsl-tests/vm_test.py")
      machine.succeed("test $(ss -ltnH | grep -c ':6001 ' || true) -eq 0")
    '';
  };
}

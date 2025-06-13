{
  inputs = {
    utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    crane.url = "github:ipetkov/crane";

    pre-commit-hooks-nix = {
      url = "github:cachix/pre-commit-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nix-github-actions = {
      url = "github:nix-community/nix-github-actions";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    utils,
    crane,
    pre-commit-hooks-nix,
    nix-github-actions,
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  in
    utils.lib.eachSystem systems (system: let
      pkgs = nixpkgs.legacyPackages."${system}";
      craneLib = crane.mkLib pkgs;
      lib = pkgs.lib;

      blueprintFilter = path: _type: builtins.match ".*blp$" path != null;
      xmlFilter = path: _type: builtins.match ".*xml$" path != null;
      jsonFilter = path: _type: builtins.match ".*json$" path != null;
      graphqlFilter = path: _type: builtins.match ".*graphql$" path != null;
      resOrCargo = path: type:
        (graphqlFilter path type) || (jsonFilter path type) || (xmlFilter path type) || (blueprintFilter path type) || (craneLib.filterCargoSources path type);

      src = lib.cleanSourceWith {
        src = ./.;
        filter = resOrCargo;
        name = "source";
      };

      common-args = {
        inherit src;
        strictDeps = true;

        buildInputs = with pkgs; [
          wayland
          libGL
          libGLU
          xorg.libX11
          xorg.libXcursor
          xorg.libXi
          xorg.libXrandr
          libxkbcommon
          fontconfig
        ];

        nativeBuildInputs = with pkgs; [
          pkg-config
          installShellFiles
          makeWrapper
        ];

        postInstall = ''
          installShellCompletion --cmd ppd-egui \
            --bash ./target/release/build/ppd-egui-*/out/ppd-egui.bash \
            --fish ./target/release/build/ppd-egui-*/out/ppd-egui.fish \
            --zsh ./target/release/build/ppd-egui-*/out/_ppd-egui
          installManPage ./target/release/build/ppd-egui-*/out/ppd-egui.1
        '';
      };

      cargoArtifacts = craneLib.buildDepsOnly common-args;

      ppd-egui = craneLib.buildPackage (common-args
        // {
          inherit cargoArtifacts;
          postFixup = ''
            wrapProgram $out/bin/ppd-egui \
              --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath common-args.buildInputs}:/run/opengl-driver/lib"
          '';
        });

      pre-commit-check = hooks:
        pre-commit-hooks-nix.lib.${system}.run {
          src = ./.;

          inherit hooks;
        };
    in rec {
      checks = {
        inherit ppd-egui;

        ppd-egui-clippy = craneLib.cargoClippy (common-args
          // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

        ppd-egui-fmt = craneLib.cargoFmt {
          inherit src;
        };

        #ppd-egui-deny = craneLib.cargoDeny {
        #  inherit src;
        #};

        pre-commit-check = pre-commit-check {
          alejandra.enable = true;
        };
      };
      packages.ppd-egui = ppd-egui;
      packages.default = packages.ppd-egui;

      apps.ppd-egui = utils.lib.mkApp {
        drv = packages.ppd-egui;
      };
      apps.default = apps.ppd-egui;

      formatter = pkgs.alejandra;

      devShells.default = let
        checks = pre-commit-check {
          alejandra.enable = true;
          rustfmt.enable = true;
          clippy.enable = true;
        };
      in
        craneLib.devShell {
          packages = with pkgs; [
            rustfmt
            clippy
            cargo-deny
            cargo-about
          ];
          inputsFrom = [ppd-egui];
          shellHook = ''
            ${checks.shellHook}
          '';
          LD_LIBRARY_PATH = "/run/opengl-driver/lib/:${lib.makeLibraryPath [pkgs.libGL pkgs.libGLU pkgs.wayland pkgs.libxkbcommon]}";
        };
    })
    // {
      hydraJobs = {
        inherit (self) checks packages devShells;
      };
      githubActions = nix-github-actions.lib.mkGithubMatrix {inherit (self) checks;};
    };
}

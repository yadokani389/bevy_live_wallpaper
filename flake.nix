{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      nixpkgs,
      flake-parts,
      ...
    }:

    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      imports = with inputs; [
        git-hooks.flakeModule
        treefmt-nix.flakeModule
      ];

      perSystem =
        {
          config,
          pkgs,
          system,
          ...
        }:
        let
          toolchain = pkgs.rust-bin.stable.latest.default;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };

          cargoDeps = rustPlatform.importCargoLock {
            lockFile = ./Cargo.lock;
          };

          example = rustPlatform.buildRustPackage {
            pname = "bevy_live_wallpaper-example";
            version = "0.2.0";
            src = ./.;

            inherit cargoDeps;

            buildFeatures = [
              "wayland"
              "x11"
            ];
            checkFeatures = [
              "wayland"
              "x11"
            ];
            cargoBuildFlags = [
              "--example=3d_shapes"
            ];

            nativeBuildInputs = with pkgs; [
              makeWrapper
              pkg-config
            ];

            buildInputs = with pkgs; [
              zstd
              libglvnd
              alsa-lib
              udev
              vulkan-loader
              wayland
              xorg.libX11
              xorg.libXcursor
              xorg.libXi
              xorg.libXrandr
            ];

            postInstall =
              let
                cargoTarget = rustPlatform.cargoInstallHook.targetSubdirectory;
              in
              ''
                install -D target/${cargoTarget}/release/examples/3d_shapes $out/bin/3d_shapes
              '';

            postFixup =
              with pkgs;
              lib.optionalString stdenv.hostPlatform.isLinux ''
                patchelf $out/bin/3d_shapes \
                  --add-rpath ${
                    lib.makeLibraryPath [
                      libxkbcommon
                      vulkan-loader
                    ]
                  }
              '';

            meta = {
              homepage = "https://github.com/yadokani389/bevy_live_wallpaper";
              license = with pkgs.lib.licenses; [
                asl20
                mit
              ];
              mainProgram = "3d_shapes";
            };
          };
        in
        {
          _module.args.pkgs = import nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };

          packages.default = example;

          devShells.default = pkgs.mkShell {
            inputsFrom = [
              config.pre-commit.devShell
            ];
            inherit (config.packages.default) nativeBuildInputs buildInputs;

            packages = with pkgs; [
              vulkan-headers
            ];

            LD_LIBRARY_PATH =
              with pkgs;
              lib.makeLibraryPath [
                libxkbcommon
                vulkan-loader
                udev
                alsa-lib
                kdePackages.wayland
                stdenv.cc.cc.lib
              ];
          };

          treefmt = {
            projectRootFile = "flake.nix";
            programs = {
              nixfmt.enable = true;
              rustfmt.enable = true;
              taplo.enable = true;
            };

            settings.formatter = {
              taplo.options = [
                "fmt"
                "-o"
                "reorder_keys=true"
              ];
            };
          };

          pre-commit = {
            check.enable = true;
            settings = {
              settings.rust.check.cargoDeps = cargoDeps;
              hooks = {
                ripsecrets.enable = true;
                typos.enable = true;
                treefmt.enable = true;
                clippy = {
                  enable = true;
                  packageOverrides.cargo = toolchain;
                  packageOverrides.clippy = toolchain;
                  settings.extraArgs = "--all-features";
                  extraPackages = example.nativeBuildInputs ++ example.buildInputs;
                };
              };
            };
          };
        };
    };
}

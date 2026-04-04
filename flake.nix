{
  description = "Jrsonnet";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/release-25.11";
    fenix = {
      url = "github:CertainLach/fenix/fix/libatomic";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    hercules-ci-effects = {
      url = "github:hercules-ci/hercules-ci-effects";
      inputs.flake-parts.follows = "flake-parts";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:CertainLach/crane/refactor/drop-remarshal";
    shelly.url = "github:CertainLach/shelly";
  };
  outputs =
    inputs:
    let
      inherit (inputs.nixpkgs.lib)
        mkIf
        mkForce
        optionals
        optionalAttrs
        ;
    in
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.shelly.flakeModule
        inputs.hercules-ci-effects.flakeModule
      ];
      systems = [
        "x86_64-linux"
        "i686-linux"
        "aarch64-linux"
        "armv7l-linux"
        "aarch64-darwin"
      ];
      perSystem =
        {
          config,
          self',
          system,
          ...
        }:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.fenix.overlays.default ];
            config.allowUnsupportedSystem = true;
            config.allowUnfreePredicate = pkg: pkg.name == "Xcode.app";
          };
          targetArch = pkgs.stdenv.hostPlatform.parsed.cpu.name;
          rustfmt = (pkgs.fenix.complete or pkgs.fenix.stable).rustfmt;
          toolchain = pkgs.fenix.combine [
            (pkgs.fenix.stable.withComponents [
              "cargo"
              "clippy"
              "rustc"
              "rust-src"
            ])
            rustfmt
          ];
          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;
          treefmt =
            (inputs.treefmt-nix.lib.evalModule pkgs (import ./treefmt.nix { inherit rustfmt; })).config.build;

          # Cross-compilation toolchains
          crossToolchain = pkgs.fenix.combine [
            (pkgs.fenix.stable.withComponents [
              "cargo"
              "rustc"
            ])
            pkgs.fenix.targets."${targetArch}-unknown-linux-musl".stable.rust-std
            pkgs.fenix.targets."${targetArch}-apple-darwin".stable.rust-std
          ];
          craneLibCross = (inputs.crane.mkLib pkgs).overrideToolchain crossToolchain;

          # Windows cross-compilation
          pkgsWindows = import inputs.nixpkgs {
            overlays = [ inputs.fenix.overlays.default ];
            localSystem = system;
            crossSystem = {
              config = "${targetArch}-w64-mingw32";
              libc = "msvcrt";
            };
          };
          windowsToolchain = pkgs.fenix.combine [
            (pkgs.fenix.stable.withComponents [
              "cargo"
              "rustc"
            ])
            pkgs.fenix.targets."${targetArch}-pc-windows-gnu".stable.rust-std
          ];
          craneLibWindows = (inputs.crane.mkLib pkgsWindows).overrideToolchain (_: windowsToolchain);

        in
        {
          legacyPackages = {
            release = optionalAttrs pkgs.stdenv.hostPlatform.isLinux (
              {
                jrsonnet-linux-glibc = self'.packages.jrsonnet;
                jrsonnet-experimental-linux-glibc = self'.packages.jrsonnet-experimental;
              }
              // optionalAttrs pkgs.stdenv.hostPlatform.is64bit rec {
                jrsonnet-linux-musl = pkgs.callPackage ./nix/jrsonnet-cross-musl.nix {
                  craneLib = craneLibCross;
                  targetTriple = "${targetArch}-unknown-linux-musl";
                  muslCC = pkgs.pkgsMusl.stdenv.cc;
                };
                jrsonnet-experimental-linux-musl = jrsonnet-linux-musl.override {
                  withExperimentalFeatures = true;
                };
              }
              // optionalAttrs (targetArch == "aarch64") rec {
                jrsonnet-darwin = pkgs.callPackage ./nix/jrsonnet-cross-darwin.nix {
                  craneLib = craneLibCross;
                  targetTriple = "${targetArch}-apple-darwin";
                };
                jrsonnet-experimental-darwin = jrsonnet-darwin.override {
                  withExperimentalFeatures = true;
                };
              }
              // optionalAttrs (targetArch == "x86_64") rec {
                jrsonnet-windows = pkgsWindows.callPackage ./nix/jrsonnet-cross-windows.nix {
                  craneLib = craneLibWindows;
                  targetTriple = "${targetArch}-pc-windows-gnu";
                };
                jrsonnet-experimental-windows = jrsonnet-windows.override {
                  withExperimentalFeatures = true;
                };
              }
            );
            benchmarks = optionalAttrs (system == "x86_64-linux" || system == "aarch64-linux") {
              default = pkgs.callPackage ./nix/benchmarks.nix {
                inherit (config.legacyPackages.jsonnetImpls)
                  go-jsonnet
                  sjsonnet
                  cpp-jsonnet
                  rsjsonnet
                  ;
                jrsonnetVariants = [
                  {
                    drv = self'.packages.jrsonnet.override { forBenchmarks = true; };
                    name = "";
                  }
                ];
              };
              quick = pkgs.callPackage ./nix/benchmarks.nix {
                inherit (config.legacyPackages.jsonnetImpls)
                  go-jsonnet
                  sjsonnet
                  cpp-jsonnet
                  rsjsonnet
                  ;
                quick = true;
                jrsonnetVariants = [
                  {
                    drv = self'.packages.jrsonnet.override { forBenchmarks = true; };
                    name = "";
                  }
                ];
              };
              against-release = pkgs.callPackage ./nix/benchmarks.nix {
                inherit (config.legacyPackages.jsonnetImpls)
                  go-jsonnet
                  sjsonnet
                  cpp-jsonnet
                  rsjsonnet
                  ;
                jrsonnetVariants = [
                  {
                    drv = self'.packages.jrsonnet.override { forBenchmarks = true; };
                    name = "current";
                  }
                  {
                    drv = self'.packages.jrsonnet-experimental.override { forBenchmarks = true; };
                    name = "current-experimental";
                  }
                  {
                    drv = self'.legacyPackages.jsonnetImpls.jrsonnet-release.override { forBenchmarks = true; };
                    name = "release";
                  }
                ];
              };
              quick-against-release = pkgs.callPackage ./nix/benchmarks.nix {
                inherit (config.legacyPackages.jsonnetImpls)
                  go-jsonnet
                  sjsonnet
                  cpp-jsonnet
                  rsjsonnet
                  ;
                quick = true;
                jrsonnetVariants = [
                  {
                    drv = self'.packages.jrsonnet.override { forBenchmarks = true; };
                    name = "current";
                  }
                  {
                    drv = self'.packages.jrsonnet-experimental.override { forBenchmarks = true; };
                    name = "current-experimental";
                  }
                  {
                    drv = self'.legacyPackages.jsonnetImpls.jrsonnet-release.override { forBenchmarks = true; };
                    name = "release";
                  }
                ];
              };
            };
            jsonnetImpls = {
              go-jsonnet = pkgs.callPackage ./nix/go-jsonnet.nix { };
              sjsonnet = pkgs.callPackage ./nix/sjsonnet.nix { };
              cpp-jsonnet = pkgs.callPackage ./nix/cpp-jsonnet.nix { };
              # I didn't managed to build it, and nixpkgs version is marked as broken
              # haskell-jsonnet = pkgs.callPackage ./nix/haskell-jsonnet.nix { };
              rsjsonnet = pkgs.callPackage ./nix/rsjsonnet.nix { };
              # Older released version of jrsonnet itself, for benchmarking purposes
              jrsonnet-release = pkgs.callPackage ./nix/jrsonnet-release.nix {
                rustPlatform = pkgs.makeRustPlatform {
                  rustc = toolchain;
                  cargo = toolchain;
                };
              };
            };
          };
          packages =
            let
              jrsonnet = pkgs.callPackage ./nix/jrsonnet.nix {
                inherit craneLib;
              };
              jrsonnet-experimental = pkgs.callPackage ./nix/jrsonnet.nix {
                inherit craneLib;
                withExperimentalFeatures = true;
              };
            in
            {
              default = jrsonnet;
              inherit jrsonnet jrsonnet-experimental;
            };
          checks = optionalAttrs (system != "armv7l-linux") {
            formatting = treefmt.check inputs.self;
          };
          formatter = mkIf (system != "armv7l-linux") treefmt.wrapper;
          shelly.shells.default = {
            factory = craneLib.devShell;
            packages =
              with pkgs;
              [
                cargo-edit
                cargo-outdated
                cargo-watch
                cargo-insta
                cargo-hack
                cargo-show-asm
                lld
                hyperfine
                graphviz
              ]
              ++ optionals (!stdenv.isDarwin) [
                valgrind
                kdePackages.kcachegrind
                samply
              ];
          };
        };
      hercules-ci.github-releases.files =
        let
          rel = system: inputs.self.legacyPackages.${system}.release;
          bin = drv: "${drv}/bin/jrsonnet";
          exe = drv: "${drv}/bin/jrsonnet.exe";
        in
        [
          {
            label = "jrsonnet-x86_64-linux-musl";
            path = bin (rel "x86_64-linux").jrsonnet-linux-musl;
          }
          {
            label = "jrsonnet-experimental-x86_64-linux-musl";
            path = bin (rel "x86_64-linux").jrsonnet-experimental-linux-musl;
          }
          {
            label = "jrsonnet-aarch64-darwin";
            path = bin (rel "aarch64-linux").jrsonnet-darwin;
          }
          {
            label = "jrsonnet-experimental-aarch64-darwin";
            path = bin (rel "aarch64-linux").jrsonnet-experimental-darwin;
          }
          {
            label = "jrsonnet-x86_64-windows.exe";
            path = exe (rel "x86_64-linux").jrsonnet-windows;
          }
          {
            label = "jrsonnet-experimental-x86_64-windows.exe";
            path = exe (rel "x86_64-linux").jrsonnet-experimental-windows;
          }

          {
            label = "jrsonnet-aarch64-linux-musl";
            path = bin (rel "aarch64-linux").jrsonnet-linux-musl;
          }
          {
            label = "jrsonnet-experimental-aarch64-linux-musl";
            path = bin (rel "aarch64-linux").jrsonnet-experimental-linux-musl;
          }

          {
            label = "jrsonnet-x86_64-linux-glibc";
            path = bin (rel "x86_64-linux").jrsonnet-linux-glibc;
          }
          {
            label = "jrsonnet-experimental-x86_64-linux-glibc";
            path = bin (rel "x86_64-linux").jrsonnet-experimental-linux-glibc;
          }
          {
            label = "jrsonnet-aarch64-linux-glibc";
            path = bin (rel "aarch64-linux").jrsonnet-linux-glibc;
          }
          {
            label = "jrsonnet-experimental-aarch64-linux-glibc";
            path = bin (rel "aarch64-linux").jrsonnet-experimental-linux-glibc;
          }
          {
            label = "jrsonnet-i686-linux-glibc";
            path = bin (rel "i686-linux").jrsonnet-linux-glibc;
          }
          {
            label = "jrsonnet-experimental-i686-linux-glibc";
            path = bin (rel "i686-linux").jrsonnet-experimental-linux-glibc;
          }
          {
            label = "jrsonnet-armv7l-linux-glibc";
            path = bin (rel "armv7l-linux").jrsonnet-linux-glibc;
          }
          {
            label = "jrsonnet-experimental-armv7l-linux-glibc";
            path = bin (rel "armv7l-linux").jrsonnet-experimental-linux-glibc;
          }
        ];
      hercules-ci.cargo-publish = {
        enable = true;
        secretName = "crates-io";
      };
      hercules-ci.flake-update = {
        enable = true;
        baseMerge.enable = true;
        baseMerge.method = "fast-forward";
        when = {
          dayOfWeek = [ "Sat" ];
        };
      };
      herculesCI =
        { lib, config, ... }:
        {
          ciSystems = [
            "x86_64-linux"
            "i686-linux"
            "aarch64-linux"
            "armv7l-linux"
            # TODO: add workers for these platforms
            # "aarch64-darwin"
          ];
          onPush.default.outputs = {
            benchmarks.x86_64-linux = inputs.self.legacyPackages.x86_64-linux.benchmarks.default;

            # Cross: musl/mingw/darwin-zigbuild
            release.x86_64-linux = inputs.self.legacyPackages.x86_64-linux.release;
            release.aarch64-linux = inputs.self.legacyPackages.aarch64-linux.release;
            release.armv7l-linux = inputs.self.legacyPackages.armv7l-linux.release;
            release.i686-linux = inputs.self.legacyPackages.i686-linux.release;

            # Too much to build for CI purposes
            devShells = mkForce { };
            formatter = mkForce { };

            # No need to run them on different arch, pretty large derivations and might try to compile GHC
            checks.i686-linux.formatting = mkForce { };
            checks.aarch64-linux.formatting = mkForce { };
          };
        };
    };
}

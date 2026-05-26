{
  description = "Jrsonnet";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/master";
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
    crane.url = "github:ipetkov/crane";
    shelly.url = "github:CertainLach/shelly";

    cpp-jsonnet-for-tests = {
      url = "github:google/jsonnet";
      flake = false;
    };
    go-jsonnet-for-tests = {
      url = "github:google/go-jsonnet";
      flake = false;
    };
  };
  outputs =
    inputs:
    let
      inherit (inputs.nixpkgs.lib)
        mkIf
        mkForce
        optionals
        optionalAttrs
        concatMapStringsSep
        ;
      rel = system: inputs.self.legacyPackages.${system}.release;
      releaseSections = [
        {
          name = "Linux (glibc)";
          artifacts = [
            {
              label = "jrsonnet-x86_64-linux-glibc";
              drv = (rel "x86_64-linux").jrsonnet-linux-glibc;
            }
            {
              label = "jrsonnet-experimental-x86_64-linux-glibc";
              drv = (rel "x86_64-linux").jrsonnet-experimental-linux-glibc;
            }
            {
              label = "jrsonnet-aarch64-linux-glibc";
              drv = (rel "aarch64-linux").jrsonnet-linux-glibc;
            }
            {
              label = "jrsonnet-experimental-aarch64-linux-glibc";
              drv = (rel "aarch64-linux").jrsonnet-experimental-linux-glibc;
            }
            {
              label = "jrsonnet-i686-linux-glibc";
              drv = (rel "i686-linux").jrsonnet-linux-glibc;
            }
            {
              label = "jrsonnet-experimental-i686-linux-glibc";
              drv = (rel "i686-linux").jrsonnet-experimental-linux-glibc;
            }
            {
              label = "jrsonnet-armv7l-linux-glibc";
              drv = (rel "armv7l-linux").jrsonnet-linux-glibc;
            }
            {
              label = "jrsonnet-experimental-armv7l-linux-glibc";
              drv = (rel "armv7l-linux").jrsonnet-experimental-linux-glibc;
            }
          ];
        }
        {
          name = "Linux (musl)";
          artifacts = [
            {
              label = "jrsonnet-x86_64-linux-musl";
              drv = (rel "x86_64-linux").jrsonnet-linux-musl;
            }
            {
              label = "jrsonnet-experimental-x86_64-linux-musl";
              drv = (rel "x86_64-linux").jrsonnet-experimental-linux-musl;
            }
            {
              label = "jrsonnet-aarch64-linux-musl";
              drv = (rel "aarch64-linux").jrsonnet-linux-musl;
            }
            {
              label = "jrsonnet-experimental-aarch64-linux-musl";
              drv = (rel "aarch64-linux").jrsonnet-experimental-linux-musl;
            }
          ];
        }
        {
          name = "macOS";
          artifacts = [
            {
              label = "jrsonnet-aarch64-darwin";
              drv = (rel "aarch64-linux").jrsonnet-darwin;
            }
            {
              label = "jrsonnet-experimental-aarch64-darwin";
              drv = (rel "aarch64-linux").jrsonnet-experimental-darwin;
            }
          ];
        }
        {
          name = "Windows";
          artifacts = [
            {
              label = "jrsonnet-x86_64-windows";
              drv = (rel "x86_64-linux").jrsonnet-windows;
              windows = true;
            }
            {
              label = "jrsonnet-experimental-x86_64-windows";
              drv = (rel "x86_64-linux").jrsonnet-experimental-windows;
              windows = true;
            }
          ];
        }
      ];
      releaseArtifacts = builtins.concatMap (s: s.artifacts) releaseSections;
    in
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.shelly.flakeModule
        inputs.hercules-ci-effects.flakeModule
        ./nix/post-comment.nix
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
              "rustfmt"
            ])
            pkgs.fenix.targets.wasm32-unknown-unknown.stable.rust-std
          ];
          devToolchain = pkgs.fenix.combine [
            ((pkgs.fenix.complete or pkgs.fenix.stable).withComponents [
              "cargo"
              "clippy"
              "rustc"
              "rust-src"
              "rustfmt"
              "rust-analyzer"
            ])
            pkgs.fenix.targets.wasm32-unknown-unknown.latest.rust-std
          ];
          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;
          craneLibDev = (inputs.crane.mkLib pkgs).overrideToolchain devToolchain;
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
            fetchJrq = pkgs.callPackage ./nix/fetch-jrq.nix {
              inherit (self'.packages) jrsonnet;
            };
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
                  # https://github.com/rust-cross/cargo-zigbuild/issues/433
                  zig = pkgs.zig_0_15;
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
                inherit (config.legacyPackages) fetchJrq;
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
                inherit (config.legacyPackages) fetchJrq;
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
                inherit (config.legacyPackages) fetchJrq;
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
                inherit (config.legacyPackages) fetchJrq;
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
                inherit (inputs) cpp-jsonnet-for-tests go-jsonnet-for-tests;
              };
              jrsonnet-experimental = pkgs.callPackage ./nix/jrsonnet.nix {
                inherit craneLib;
                inherit (inputs) cpp-jsonnet-for-tests go-jsonnet-for-tests;
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
            factory = craneLibDev.devShell;
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
            environment = {
              CPP_JSONNET_FOR_TESTS = inputs.cpp-jsonnet-for-tests;
              GO_JSONNET_FOR_TESTS = inputs.go-jsonnet-for-tests;
            };
          };
          shelly.shells.impls = {
            packages =
              (with self'.legacyPackages.jsonnetImpls; [
                cpp-jsonnet
                go-jsonnet
                rsjsonnet
                sjsonnet
              ])
              ++ (with self'.packages; [
                jrsonnet
              ]);
          };
        };
      hercules-ci.github-releases.files = map (a: {
        label = a.label + (if a.windows or false then ".exe" else "");
        path = "${a.drv}/bin/jrsonnet${if a.windows or false then ".exe" else ""}";
      }) releaseArtifacts;
      hercules-ci.cargo-publish = {
        enable = true;
        secretName = "crates-io";
        extraPublishArgs = [ "--workspace" ];
        assertVersions = true;
      };
      hercules-ci.flake-update = {
        enable = true;
        baseMerge.enable = true;
        baseMerge.method = "fast-forward";
        when = {
          dayOfWeek = [ "Sat" ];
        };
      };
      hercules-ci.post-comment = {
        enable = true;
        caches = [ "cache.delta.rocks/hercules" ];
        script =
          let
            benchmarks = inputs.self.legacyPackages.x86_64-linux.benchmarks.default;
            renderSection = s: ''
              echo
              echo "### ${s.name}"
              echo
              ${concatMapStringsSep "\n" (a: ''echo "- [${a.label}]($(nixTar ${a.drv}))"'') s.artifacts}
            '';
          in
          ''
            {
              echo "## Benchmark results"
              echo
              echo "[View rendered]($(nixRender ${benchmarks}))"
              echo
              echo "## Downloads"
              ${concatMapStringsSep "\n" renderSection releaseSections}
            } > $out
          '';
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

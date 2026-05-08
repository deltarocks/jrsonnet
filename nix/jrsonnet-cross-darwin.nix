{
  lib,
  craneLib,
  cargo-zigbuild,
  zig,
  darwin,
  targetTriple,
  withExperimentalFeatures ? false,
}:
let
  inherit (lib) optionalString;
in
craneLib.buildPackage {
  src = lib.cleanSourceWith {
    src = ../.;
    filter = path: type: (lib.hasSuffix ".jsonnet" path) || (craneLib.filterCargoSources path type);
  };
  pname = "jrsonnet";
  version = "current${optionalString withExperimentalFeatures "-experimental"}";
  strictDeps = true;

  depsBuildBuild = [
    zig
    cargo-zigbuild
  ];

  nativeBuildInputs = [
    darwin.xcode_12_2
  ];

  cargoExtraArgs = "-p jrsonnet";

  buildPhaseCargoCommand = ''
    export SDKROOT=${darwin.xcode_12_2}/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk
    export XDG_CACHE_HOME=$TMPDIR/xdg_cache
    mkdir -p $XDG_CACHE_HOME
    export CARGO_ZIGBUILD_CACHE_DIR=$TMPDIR/cargo-zigbuild-cache
    mkdir -p $CARGO_ZIGBUILD_CACHE_DIR

    HOME=$(mktemp -d)
    cargo zigbuild --release --locked ${optionalString withExperimentalFeatures "--features=experimental"} --target=${targetTriple}
  '';

  doNotPostBuildInstallCargoBinaries = true;
  installPhaseCommand = ''
    mkdir -p $out/bin $out/lib
    cp target/${targetTriple}/release/jrsonnet $out/bin/jrsonnet
    cp target/${targetTriple}/release/jrsonnet-fmt $out/bin/jrsonnet-fmt
    cp target/${targetTriple}/release/jrb $out/bin/jrb
    cp target/${targetTriple}/release/libjsonnet.dylib $out/lib/libjsonnet.dylib
    cp target/${targetTriple}/release/libjsonnet.a $out/lib/libjsonnet.a
  '';
}

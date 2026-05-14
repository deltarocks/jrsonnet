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
    mkdir -p $out/{bin,lib}
    cp target/${targetTriple}/release/{jrsonnet,jrsonnet-fmt,jrb} $out/bin/
    cp target/${targetTriple}/release/{libjsonnet.dylib,libjsonnet.a} $out/lib/
  '';
}

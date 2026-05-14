{
  lib,
  craneLib,
  muslCC,
  targetTriple,
  withExperimentalFeatures ? false,
}:
let
  targetEnv = builtins.replaceStrings [ "-" ] [ "_" ] targetTriple;
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

  cargoExtraArgs = "--locked --features=mimalloc${optionalString withExperimentalFeatures ",experimental"} --target=${targetTriple}";

  "CC_${targetEnv}" = "${muslCC}/bin/${muslCC.targetPrefix}cc";
  "CARGO_TARGET_${lib.toUpper targetEnv}_LINKER" = "${muslCC}/bin/${muslCC.targetPrefix}cc";

  doNotPostBuildInstallCargoBinaries = true;
  installPhaseCommand = ''
    mkdir -p $out/bin
    cp target/${targetTriple}/release/{jrsonnet,jrsonnet-fmt,jrb} $out/bin/
  '';
}

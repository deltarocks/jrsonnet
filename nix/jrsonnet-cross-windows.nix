{
  lib,
  craneLib,
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

  cargoExtraArgs = "--locked ${optionalString withExperimentalFeatures "--features=experimental"} --target=${targetTriple}";

  doNotPostBuildInstallCargoBinaries = true;
  installPhaseCommand = ''
    mkdir -p $out/{bin,lib}
    ls target/${targetTriple}/release/
    cp target/${targetTriple}/release/{jrsonnet.exe,jrsonnet-fmt.exe,jrb.exe} $out/bin/
    cp target/${targetTriple}/release/{jsonnet.dll,libjsonnet.dll.a} $out/lib/
  '';
}

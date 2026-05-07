{
  lib,
  stdenvNoCC,
  cacert,
  jrsonnet,
}:
{
  lockfile,
  vendorHash,
  name ? "jrq-vendor",
}:
stdenvNoCC.mkDerivation (finalAttrs: {
  inherit name;

  outputHashMode = "recursive";
  outputHashAlgo = "sha256";
  outputHash = vendorHash;

  nativeBuildInputs = [
    jrsonnet
    cacert
  ];

  dontUnpack = true;
  dontConfigure = true;
  dontInstall = true;
  dontFixup = true;

  SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";
  GIT_SSL_CAINFO = "${cacert}/etc/ssl/certs/ca-bundle.crt";

  buildPhase = ''
    runHook preBuild

    export HOME=$TMPDIR

    install -m644 ${lockfile} jsonnetfile.json
    install -m644 ${lockfile} jsonnetfile.lock.json

    mkdir -p "$out"
    jrb --jsonnetpkg-home "$out" install

    runHook postBuild
  '';

  passthru = {
    inherit lockfile vendorHash;
  };
})

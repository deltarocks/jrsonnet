{
  lib,
  runCommand,
  stdenv,
  fetchJrq,
  go-jsonnet,
  sjsonnet,
  cpp-jsonnet,
  rsjsonnet,
  hyperfine,
  quick ? false,
  jrsonnetVariants,
}:
with lib;
let
  inherit (cpp-jsonnet) jsonnetBench;
  inherit (go-jsonnet) goJsonnetBench;
  realworldVendor = fetchJrq {
    name = "realworld-vendor";
    lockfile = ../tests/realworld/jsonnetfile.lock.json;
    vendorHash = "sha256-oEUzM6Bhu8ZT8vCtYDbBEjG5BFHYpID+1/2pgXvIAgo=";
  };
  realworldBench = runCommand "realworld-bench" { } ''
    mkdir -p $out
    cp ${../tests/realworld}/*.jsonnet ${../tests/realworld}/*.libsonnet $out/
    cp -r ${realworldVendor} $out/vendor
  '';

  # Removes outsiders from the output
  # Useful when comparing performance of different jrsonnet releases
  skipSlow = if quick then "slow benchmark, but only quick requested" else "";
in
stdenv.mkDerivation {
  name = "benchmarks";
  # __impure = true; # not supported by hercules-ci
  unpackPhase = "true";

  buildInputs = [
    sjsonnet
    cpp-jsonnet
    rsjsonnet
    go-jsonnet

    hyperfine
  ];

  installPhase =
    let
      mkBench =
        {
          name,
          path,
          omitSource ? false,
          pathIsGenerator ? false,
          skipRustAlternative ? "",
          skipScala ? "",
          skipCpp ? "",
          skipGo ? "",
          jpaths ? [ ],
        }:
        let
          jpathArgs = concatMapStrings (p: " -J ${p}") jpaths;
        in
        ''
          echo >> $out
          echo "=== ${name}" >> $out
          echo >> $out
          ${optionalString (skipRustAlternative != "") ''
            echo "> Note: No results for Rust (alternative), ${skipRustAlternative}" >> $out
            echo >> $out
          ''}
          ${optionalString (skipGo != "") ''
            echo "> Note: No results for Go, ${skipGo}" >> $out
            echo >> $out
          ''}
          ${optionalString (skipScala != "") ''
            echo "> Note: No results for Scala (native)/Scala (GraalVM), ${skipScala}" >> $out
            echo >> $out
          ''}
          ${optionalString (skipCpp != "") ''
            echo "> Note: No results for C++, ${skipCpp}" >> $out
            echo >> $out
          ''}
          ${optionalString (!omitSource) ''
            echo ".Source" >> $out
            echo "[%collapsible]" >> $out
            echo "====" >> $out
            echo "[source,jsonnet]" >> $out
            echo "----" >> $out
            ${optionalString pathIsGenerator "echo \"// Generator source\" >> $out"}
            cat ${path} >> $out
            echo >> $out
            echo "----" >> $out
            echo "====" >> $out
            echo >> $out
          ''}
          path=${path}
          ${optionalString pathIsGenerator ''
            go-jsonnet $path > generated.jsonnet
            path=generated.jsonnet
          ''}
          hyperfine -N -w4 -m20 --output=pipe --style=basic --export-asciidoc result.adoc \
            ${
              concatStringsSep " " (
                forEach jrsonnetVariants (
                  variant:
                  "\"${variant.drv}/bin/jrsonnet $path${jpathArgs}\" -n \"Rust${
                    if variant.name != "" then " (${variant.name})" else ""
                  }\""
                )
              )
            } \
            ${
              optionalString (
                skipRustAlternative == ""
              ) "\"rsjsonnet $path${jpathArgs}\" -n \"Rust (alternative, rsjsonnet)\""
            } \
            ${optionalString (skipGo == "") "\"go-jsonnet $path${jpathArgs}\" -n \"Go\""} \
            ${
              optionalString (skipScala == "") "\"sjsonnet-native $path${jpathArgs}\" -n \"Scala (native)\""
            } \
            ${
              # My aarch64-linux machine can't run graalvm image:
              # The current machine does not support all of the following CPU features that are required by the image: [FP, ASIMD, CRC32, LSE].
              optionalString (
                skipScala == "" && stdenv.hostPlatform.system != "aarch64-linux"
              ) "\"sjsonnet-graalvm $path${jpathArgs}\" -n \"Scala (GraalVM)\""
            } \
            ${optionalString (skipCpp == "") "\"jsonnet $path${jpathArgs}\" -n \"C++\""}
          cat result.adoc >> $out
        '';
    in
    ''
      set -oux
      ulimit -s 65535

      temp=$(mktemp -d)
      cd $temp

      touch $out
      ${optionalString (true) ''
        cat ${./benchmarks.adoc} >> $out
        echo >> $out

        echo "CPU: $(grep 'model name' /proc/cpuinfo | head -1 | cut -d: -f2 | xargs), $(grep -c '^processor' /proc/cpuinfo) threads" >> $out
        echo >> $out

        echo ".Tested versions" >> $out
        echo "[%collapsible]" >> $out
        echo "====" >> $out
        echo "* Go: $(go-jsonnet --version)" >> $out
        echo "* C++: $(jsonnet --version)" >> $out
        echo "* Scala (native/GraalVM): $(sjsonnet-native 2>&1 | grep -oP 'Sjsonnet \S+')" >> $out
        echo "* Rust (alternative): rsjsonnet ${rsjsonnet.version} (${rsjsonnet.src.rev})" >> $out
        ${concatStringsSep "\n" (
          forEach jrsonnetVariants (
            variant:
            "echo \"* Rust${
              if variant.name != "" then " (${variant.name})" else ""
            }: $(${variant.drv}/bin/jrsonnet --version 2>&1)\" >> $out"
          )
        )}
        echo "====" >> $out
        echo >> $out
      ''}
      echo "== Real world" >> $out
      ${mkBench {
        name = "GitLab runbooks dashboards";
        path = "${realworldBench}/entry-gitlab-runbooks.jsonnet";
        jpaths = [
          "${realworldBench}/vendor"
          "${realworldBench}/vendor/runbooks/libsonnet"
          "${realworldBench}/vendor/runbooks/dashboards"
          "${realworldBench}/vendor/runbooks/services"
          "${realworldBench}/vendor/runbooks/metrics-catalog"
        ];
        skipCpp = "too slow, takes hours, skews results";
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "GraalVM CI";
        path = "${realworldBench}/entry-graalvm.jsonnet";
        jpaths = [
          "${realworldBench}/vendor/graal"
        ];
        skipCpp = "too slow, takes hours, skews results";
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "Kube-prometheus";
        path = "${realworldBench}/entry-kube-prometheus.jsonnet";
        jpaths = [
          "${realworldBench}/vendor"
        ];
        skipCpp = "too slow, takes hours, skews results";
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "Loki manifests";
        path = "${realworldBench}/entry-loki.jsonnet";
        jpaths = [
          "${realworldBench}/vendor"
          "${realworldBench}"
        ];
        skipCpp = "too slow, takes hours, skews results";
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "Mimir manifests";
        path = "${realworldBench}/entry-mimir.jsonnet";
        jpaths = [
          "${realworldBench}/vendor"
          "${realworldBench}"
        ];
        skipCpp = "too slow, takes hours, skews results";
        skipGo = skipSlow;
        skipScala = "https://github.com/databricks/sjsonnet/issues/829";
      }}
      ${mkBench {
        name = "Tempo manifests";
        path = "${realworldBench}/entry-tempo.jsonnet";
        jpaths = [
          "${realworldBench}/vendor"
          "${realworldBench}"
        ];
        skipCpp = "too slow, takes hours, skews results";
        skipGo = skipSlow;
      }}

      echo >> $out
      echo "== Benchmarks from C++ jsonnet (/perf_tests)" >> $out
      ${mkBench {
        name = "Large string join";
        path = "${jsonnetBench}/perf_tests/large_string_join.jsonnet";
      }}
      ${mkBench {
        name = "Large string template";
        omitSource = true;
        path = "${jsonnetBench}/perf_tests/large_string_template.jsonnet";
        skipGo = "fails with os stack size exhausion";
        skipCpp = "too slow, takes hours, skews results";
      }}
      ${mkBench {
        name = "Realistic 1";
        path = "${jsonnetBench}/perf_tests/realistic1.jsonnet";
        skipGo = skipSlow;
        skipCpp = "too slow, takes hours, skews results";
      }}
      ${mkBench {
        name = "Realistic 2";
        path = "${jsonnetBench}/perf_tests/realistic2.jsonnet";
        skipGo = skipSlow;
        skipCpp = "too slow, takes hours, skews results";
      }}

      echo >> $out
      echo "== Benchmarks from C++ jsonnet (/benchmarks)" >> $out
      ${mkBench {
        name = "Tail call";
        path = "${jsonnetBench}/benchmarks/bench.01.jsonnet";
      }}
      ${mkBench {
        name = "Inheritance recursion";
        path = "${jsonnetBench}/benchmarks/bench.02.jsonnet";
        skipCpp = skipSlow;
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "Simple recursive call";
        path = "${jsonnetBench}/benchmarks/bench.03.jsonnet";
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "Foldl string concat";
        path = "${jsonnetBench}/benchmarks/bench.04.jsonnet";
        skipCpp = skipSlow;
      }}
      ${mkBench {
        name = "Array sorts";
        path = "${jsonnetBench}/benchmarks/bench.06.jsonnet";
        skipCpp = skipSlow;
      }}
      ${mkBench {
        name = "Lazy array";
        path = "${jsonnetBench}/benchmarks/bench.07.jsonnet";
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "Inheritance function recursion";
        path = "${jsonnetBench}/benchmarks/bench.08.jsonnet";
        skipCpp = skipSlow;
      }}
      ${mkBench {
        name = "String strips";
        path = "${jsonnetBench}/benchmarks/bench.09.jsonnet";
        skipCpp = "too slow, takes hours, skews results";
      }}
      ${mkBench {
        name = "Big object";
        path = "${jsonnetBench}/benchmarks/gen_big_object.jsonnet";
        pathIsGenerator = true;
      }}

      echo >> $out
      echo "== Benchmarks from Go jsonnet (builtins)" >> $out
      ${mkBench {
        name = "std.base64";
        path = "${goJsonnetBench}/base64.jsonnet";
        skipRustAlternative = skipSlow;
        skipCpp = "too slow, takes hours, skews results";
      }}
      ${mkBench {
        name = "std.base64Decode";
        path = "${goJsonnetBench}/base64Decode.jsonnet";
        skipRustAlternative = skipSlow;
        skipCpp = skipSlow;
      }}
      ${mkBench {
        name = "std.base64DecodeBytes";
        path = "${goJsonnetBench}/base64DecodeBytes.jsonnet";
        skipRustAlternative = skipSlow;
        skipCpp = skipSlow;
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "std.base64 (byte array)";
        path = "${goJsonnetBench}/base64_byte_array.jsonnet";
        skipRustAlternative = skipSlow;
        skipCpp = skipSlow;
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "std.foldl";
        path = "${goJsonnetBench}/foldl.jsonnet";
      }}
      ${mkBench {
        name = "std.manifestJsonEx";
        path = "${goJsonnetBench}/manifestJsonEx.jsonnet";
        skipCpp = skipSlow;
      }}
      ${mkBench {
        name = "std.manifestTomlEx";
        path = "${goJsonnetBench}/manifestTomlEx.jsonnet";
        skipCpp = skipSlow;
      }}
      ${mkBench {
        name = "std.parseInt";
        path = "${goJsonnetBench}/parseInt.jsonnet";
        skipCpp = skipSlow;
      }}
      ${mkBench {
        name = "std.reverse";
        path = "${goJsonnetBench}/reverse.jsonnet";
        skipCpp = skipSlow;
        skipGo = skipSlow;
      }}
      ${mkBench {
        name = "std.substr";
        path = "${goJsonnetBench}/substr.jsonnet";
      }}
      ${mkBench {
        name = "Comparsion for array";
        path = "${goJsonnetBench}/comparison.jsonnet";
        skipCpp = "too slow, takes hours, skews results";
      }}
      ${mkBench {
        name = "Comparsion for primitives";
        path = "${goJsonnetBench}/comparison2.jsonnet";
        skipRustAlternative = skipSlow;
        skipCpp = "can't run: uses up to 192GB of RAM";
        skipGo = skipSlow;
      }}
    '';
}

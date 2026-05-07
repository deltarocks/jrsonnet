local common = import 'ci/ci_common/common.jsonnet';
local graal_common = import 'graal-common.json';

local compiler = import 'compiler/ci/ci.jsonnet';
local wasm = import 'wasm/ci/ci.jsonnet';
local espresso = import 'espresso/ci/ci.jsonnet';
local regex = import 'regex/ci/ci.jsonnet';
local sdk = import 'sdk/ci/ci.jsonnet';
local substratevm = import 'substratevm/ci/ci.jsonnet';
local sulong = import 'sulong/ci/ci.jsonnet';
local tools = import 'tools/ci/ci.jsonnet';
local truffle = import 'truffle/ci/ci.jsonnet';
local javadoc = import 'ci_includes/publish-javadoc.jsonnet';
local visualizer = import 'visualizer/ci/ci.jsonnet';
local web_image = import 'web-image/ci/ci.jsonnet';

{
  ci_resources:: (import 'ci/ci_common/ci-resources.libsonnet'),
  overlay: graal_common.ci.overlay,
  specVersion: '7',
  tierConfig: {
    tier1: 'gate',
    tier2: 'gate',
    tier3: 'gate',
    tier4: 'post-merge',
  },
  builds: [common.add_excludes_guard(common.with_style_component(b)) for b in (
    common.with_components(compiler.builds, ['compiler']) +
    common.with_components(wasm.builds, ['wasm']) +
    common.with_components(espresso.builds, ['espresso']) +
    common.with_components(regex.builds, ['regex']) +
    common.with_components(sdk.builds, ['sdk']) +
    common.with_components(substratevm.builds, ['svm']) +
    common.with_components(sulong.builds, ['sulong']) +
    common.with_components(tools.builds, ['tools']) +
    common.with_components(truffle.builds, ['truffle']) +
    common.with_components(javadoc.builds, ['javadoc']) +
    common.with_components(visualizer.builds, ['visualizer']) +
    common.with_components(web_image.builds, ['webimage'])
  )],
  assert (import 'ci/ci_common/run-spec-demo.jsonnet').check(),
}

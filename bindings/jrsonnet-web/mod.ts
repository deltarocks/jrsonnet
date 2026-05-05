import { assert } from "@std/assert";
import {
  format as formatRaw,
  type ImportResolver,
  WasmFormatOptions,
  WasmState,
  WasmVal,
} from "./lib/jsonnet_web.js";

export { type ImportResolver, WasmFormatOptions, WasmState, WasmVal };

class FetchImportResolver implements ImportResolver {
  constructor(public base: string) {}

  resolution = new Map<string, URL>();
  response = new Map<string, Response>();

  async resolveFrom(from: string | undefined, path: string): Promise<string> {
    let resolved: URL;
    if (from) {
      resolved = new URL(path, from);
    } else {
      resolved = new URL(path, this.base);
    }
    const resolvingStr = resolved.toString();
    resolved = this.resolution.get(resolvingStr) ?? resolved;

    const resolvedStr = resolved.toString();
    if (!this.response.has(resolvedStr)) {
      console.log(resolved);
      const v = await fetch(resolved);
      this.response.set(resolvedStr, v);
      resolved = new URL(v.url);
      this.resolution.set(resolvingStr, resolved);
    }
    return resolved.toString();
  }
  loadFileContents(resolved: string): Promise<Uint8Array> {
    console.log(resolved);
    const v = this.response.get(resolved);
    assert(v, "should be resolved");
    return v.bytes();
  }
}

//
// try {
//   console.log("eval file");
//   await state.evaluate_file("example.jsonnet");
//   console.log("eval file done");
// } catch (e) {
//   console.log(e);
// }
//
export function format(
  code: string,
  opts: WasmFormatOptions = new WasmFormatOptions(),
): string {
  return formatRaw(code, opts);
}

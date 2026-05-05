import { assertEquals } from "@std/assert";
import { WasmState } from "./mod.ts";

Deno.test("basic", () => {
  const state = new WasmState();

  assertEquals(state.evaluate_snippet("test.jsonnet", "1 + 2").as_num(), 3);
});

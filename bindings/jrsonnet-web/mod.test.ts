import { assert, assertEquals, assertRejects, assertThrows } from "@std/assert";
import {
	collectGarbage,
	type ImportResolver,
	JrsonnetError,
	State,
	ValKind,
} from "./mod.ts";

Deno.test("evaluateSnippet returns numbers", () => {
	const state = new State();
	const v = state.evaluateSnippet("test.jsonnet", "1 + 2");
	assertEquals(v.kind, ValKind.Num);
	assertEquals(v.asNum(), 3);
});

Deno.test("evaluateSnippet returns booleans", () => {
	const state = new State();
	const v = state.evaluateSnippet("test.jsonnet", "true && !false");
	assertEquals(v.kind, ValKind.Bool);
	assertEquals(v.asBool(), true);
});

Deno.test("evaluateSnippet returns strings", () => {
	const state = new State();
	const v = state.evaluateSnippet("test.jsonnet", "'hello ' + 'world'");
	assertEquals(v.kind, ValKind.Str);
	assertEquals(v.asString(), "hello world");
});

Deno.test("evaluateSnippet returns null", () => {
	const state = new State();
	const v = state.evaluateSnippet("test.jsonnet", "null");
	assertEquals(v.kind, ValKind.Null);
	assertEquals(v.asNum(), undefined);
});

Deno.test("Val.asArr exposes ArrValue", () => {
	const state = new State();
	const arr = state.evaluateSnippet("test.jsonnet", "[10, 20, 30]").asArr();
	if (!arr) throw new Error("expected array");
	assertEquals(arr.length, 3);
	assertEquals(arr.at(1)?.asNum(), 20);
	assertEquals(arr.at(99), undefined);
});

Deno.test("Val.asObj exposes ObjValue", () => {
	const state = new State();
	const obj = state.evaluateSnippet("test.jsonnet", "{a: 1, b: 'two'}").asObj();
	if (!obj) throw new Error("expected object");
	assertEquals(obj.keys().sort(), ["a", "b"]);
	assertEquals(obj.get("a")?.asNum(), 1);
	assertEquals(obj.get("b")?.asString(), "two");
	assertEquals(obj.get("missing"), undefined);
});

Deno.test("evaluateSnippet manifests JSON", () => {
	const state = new State();
	const v = state.evaluateSnippet("test.jsonnet", "{a: 1, b: [2, 3]}");
	assertEquals(v.manifestJson(0), '{"a":1,"b":[2,3]}');
});

Deno.test("evaluateSnippet propagates jsonnet errors", () => {
	const state = new State();
	assertThrows(() => state.evaluateSnippet("test.jsonnet", "error 'boom'"));
});

Deno.test("evaluateFile without resolver rejects", async () => {
	const state = new State();
	await assertRejects(() => state.evaluateFile("anything.jsonnet"));
});

Deno.test("resolver errors become JrsonnetError with cause", async () => {
	const original = new Error("disk on fire");
	const resolver: ImportResolver = {
		resolveFrom(_from, path) {
			return Promise.resolve(`memory:///${path}`);
		},
		loadFileContents(_resolved) {
			throw original;
		},
	};
	const state = new State(resolver);
	const err = await assertRejects(
		() => state.evaluateFile("anything.jsonnet"),
		JrsonnetError,
		"loadFileContents",
	);
	assertEquals(err.cause, original);
	assertEquals(err.frames[0]?.desc, "loadFileContents");
	// The wrapped error's own stack must not mention internal wasm frames.
	assertEquals((err.stack ?? "").includes(".wasm"), false);
});

Deno.test("Val.applyTla calls function with named args", () => {
	const state = new State();
	const fn = state.evaluateSnippet(
		"test.jsonnet",
		"function(x, y) x + y",
	);
	const result = fn.applyTla({
		x: state.evaluateSnippet("x.jsonnet", "10"),
		y: state.evaluateSnippet("y.jsonnet", "32"),
	});
	assertEquals(result.asNum(), 42);
});

Deno.test("Val.applyTla borrows args without consuming them", () => {
	const state = new State();
	const fn = state.evaluateSnippet("test.jsonnet", "function(x) x * 2");
	const x = state.evaluateSnippet("x.jsonnet", "21");
	assertEquals(fn.applyTla({ x }).asNum(), 42);
	assertEquals(x.asNum(), 21);
	assertEquals(fn.applyTla({ x }).asNum(), 42);
});

Deno.test("Val.applyTla on non-function returns the value unchanged", () => {
	const state = new State();
	const v = state.evaluateSnippet("test.jsonnet", "123");
	assertEquals(v.applyTla({}).asNum(), 123);
});

Deno.test("evaluateFileFrom resolves relative paths", async () => {
	const files: Record<string, string> = {
		"memory:///root/main.jsonnet": "import 'lib.jsonnet'",
		"memory:///root/lib.jsonnet": "{ answer: 42 }",
	};
	const resolver: ImportResolver = {
		resolveFrom(from, path) {
			const base = from ?? "memory:///root/";
			return Promise.resolve(new URL(path, base).toString());
		},
		loadFileContents(resolved) {
			const code = files[resolved];
			if (code === undefined) throw new Error(`missing ${resolved}`);
			return Promise.resolve(new TextEncoder().encode(code));
		},
	};
	const state = new State(resolver);
	const v = await state.evaluateFileFrom(
		"memory:///root/main.jsonnet",
		"./lib.jsonnet",
	);
	assertEquals(v.asObj()?.get("answer")?.asNum(), 42);
});

Deno.test("collectGarbage frees the reference cycles of freed states", () => {
	collectGarbage();
	const state = new State();
	const obj = state.evaluateSnippet("test.jsonnet", "{ a: self.b, b: 1 }");
	// Manifesting caches the fields, which closes the cycles.
	assertEquals(obj.manifestJson(0), '{"a":1,"b":1}');
	obj.free();
	state.free();
	assert(collectGarbage() > 0);
	assertEquals(collectGarbage(), 0);
});

Deno.test("collectGarbage keeps live states usable", () => {
	const state = new State();
	const obj = state.evaluateSnippet("test.jsonnet", "{ a: self.b, b: 1 }");
	collectGarbage();
	assertEquals(obj.asObj()?.get("a")?.asNum(), 1);
	assertEquals(
		state.evaluateSnippet("test.jsonnet", "std.length([1, 2])").asNum(),
		2,
	);
});

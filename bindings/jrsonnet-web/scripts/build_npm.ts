import { build, emptyDir } from "@deno/dnt";

await emptyDir("./npm");

await build({
	entryPoints: ["./mod.ts"],
	outDir: "./npm",
	shims: {
		// see JS docs for overview and more options
		deno: true,
	},
	package: {
		// package.json properties
		name: "jrsonnet",
		version: Deno.args[0],
		description: "Jrsonnet.",
		license: "MIT",
		repository: {
			type: "git",
			url: "git+https://github.com/deltarocks/jrsonnet.git",
		},
		bugs: {
			url: "https://github.com/deltarocks/jrsonnet/issues",
		},
	},
	postBuild() {
		Deno.copyFileSync("../../LICENSE", "npm/LICENSE");
	},
});

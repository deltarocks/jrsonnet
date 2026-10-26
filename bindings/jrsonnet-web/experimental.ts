import {
	FetchImportResolver,
	JrsonnetError,
	type JrsonnetFrame,
} from "./base.ts";
import {
	ArrValue,
	collectGarbage,
	type ImportResolver,
	ObjValue,
	setErrorFactory,
	State,
	Val,
	ValKind,
} from "./lib-exp/jsonnet_web.js";

setErrorFactory(
	(message: string, frames: JrsonnetFrame[], cause: unknown) =>
		new JrsonnetError(message, frames, cause),
);

export {
	ArrValue,
	collectGarbage,
	FetchImportResolver,
	type ImportResolver,
	JrsonnetError,
	ObjValue,
	State,
	Val,
	ValKind,
};

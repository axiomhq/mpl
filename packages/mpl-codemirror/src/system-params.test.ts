import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { mplSystemParams, type MplSystemParam } from "./system-params";

describe("mplSystemParams facet", () => {
  it("defaults to an empty array when no provider is registered", () => {
    // The facet must default to `[]` so existing consumers — who never call
    // `mplSystemParams.of(...)` — see exactly the pre-feature behaviour and
    // the wasm bridge receives an empty array (not undefined).
    const state = EditorState.create({ doc: "" });
    expect(state.facet(mplSystemParams)).toEqual([]);
  });

  it("returns the registered array when a single provider is supplied", () => {
    const params: MplSystemParam[] = [{ name: "__interval", type: "Duration" }];
    const state = EditorState.create({
      doc: "",
      extensions: [mplSystemParams.of(params)],
    });
    expect(state.facet(mplSystemParams)).toEqual(params);
  });

  it("flattens entries from multiple providers", () => {
    // Layered registrations (e.g. global query-service params + a widget's
    // extras) must compose without either side knowing about the other.
    const state = EditorState.create({
      doc: "",
      extensions: [
        mplSystemParams.of([{ name: "__interval", type: "Duration" }]),
        mplSystemParams.of([{ name: "__resolution", type: "Duration" }]),
      ],
    });
    const flat = state.facet(mplSystemParams);
    expect(flat.map(p => p.name)).toEqual(["__interval", "__resolution"]);
  });

  it("preserves the optional flag through the facet", () => {
    const state = EditorState.create({
      doc: "",
      extensions: [
        mplSystemParams.of([{ name: "__env", type: "string", optional: true }]),
      ],
    });
    const [p] = state.facet(mplSystemParams);
    expect(p.optional).toBe(true);
  });
});

import { describe, it, expect, vi } from "vitest";
import type { EditorView } from "@codemirror/view";
import {
  mapDiagnostics,
  type WasmDiagnosticItem,
} from "./diagnostics";

describe("mapDiagnostics", () => {
  it("returns an empty array when given no items", () => {
    expect(mapDiagnostics([])).toEqual([]);
  });

  it("forwards error severity from the wasm payload", () => {
    const items: WasmDiagnosticItem[] = [
      { from: 0, to: 3, severity: "error", message: "boom" },
    ];
    const [d] = mapDiagnostics(items);
    expect(d.severity).toBe("error");
    expect(d.from).toBe(0);
    expect(d.to).toBe(3);
    expect(d.message).toBe("boom");
  });

  it("preserves warning severity so the editor can highlight it", () => {
    // Locks in that parser-emitted Warnings (e.g. OldDuration) survive the
    // wasm -> CodeMirror translation as `warning`, not as `error` or `info`.
    const items: WasmDiagnosticItem[] = [
      {
        from: 10,
        to: 18,
        severity: "warning",
        message: "`duration` is deprecated; use `Duration`",
      },
    ];
    const [d] = mapDiagnostics(items);
    expect(d.severity).toBe("warning");
  });

  it("appends help text on its own line in the message", () => {
    const items: WasmDiagnosticItem[] = [
      {
        from: 0,
        to: 1,
        severity: "warning",
        message: "main",
        help: "hint",
      },
    ];
    const [d] = mapDiagnostics(items);
    expect(d.message).toBe("main\nhint");
  });

  it("widens a zero-width span to at least one character so CM renders it", () => {
    // CodeMirror collapses zero-width diagnostics. The wasm side emits
    // zero-width spans for things like "missing metric name after dataset",
    // and we want them visible.
    const items: WasmDiagnosticItem[] = [
      { from: 5, to: 5, severity: "error", message: "missing" },
    ];
    const [d] = mapDiagnostics(items);
    expect(d.from).toBe(5);
    expect(d.to).toBe(6);
  });

  it("leaves undefined actions undefined (no empty-array surprises)", () => {
    const items: WasmDiagnosticItem[] = [
      { from: 0, to: 1, severity: "warning", message: "x" },
    ];
    const [d] = mapDiagnostics(items);
    expect(d.actions).toBeUndefined();
  });

  it("returns undefined actions when the wasm payload has an empty actions array", () => {
    const items: WasmDiagnosticItem[] = [
      { from: 0, to: 1, severity: "warning", message: "x", actions: [] },
    ];
    const [d] = mapDiagnostics(items);
    expect(d.actions).toBeUndefined();
  });

  it("maps the OldDuration quick-fix to a CodeMirror Action that replaces the span", () => {
    // This is the contract the editor relies on for the OldDuration warning:
    // an action labelled with "Duration", whose apply() dispatches a change
    // covering exactly the `duration` token and inserting `Duration`.
    const items: WasmDiagnosticItem[] = [
      {
        from: 10,
        to: 18,
        severity: "warning",
        message: "`duration` is deprecated; use `Duration`",
        actions: [
          {
            name: "Replace with `Duration`",
            from: 10,
            to: 18,
            insert: "Duration",
          },
        ],
      },
    ];
    const [d] = mapDiagnostics(items);
    expect(d.actions).toBeDefined();
    expect(d.actions).toHaveLength(1);
    const action = d.actions![0];
    expect(action.name).toBe("Replace with `Duration`");

    // Verify apply() dispatches the right change set.
    const dispatch = vi.fn();
    const fakeView = { dispatch } as unknown as EditorView;
    action.apply(fakeView, 0, 0);
    expect(dispatch).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledWith({
      changes: { from: 10, to: 18, insert: "Duration" },
    });
  });

  it("maps multiple actions, preserving order and per-action spans", () => {
    const items: WasmDiagnosticItem[] = [
      {
        from: 0,
        to: 3,
        severity: "error",
        message: "typo",
        actions: [
          { name: "Replace with `rate`", from: 0, to: 3, insert: "rate" },
          { name: "Replace with `irate`", from: 0, to: 3, insert: "irate" },
        ],
      },
    ];
    const [d] = mapDiagnostics(items);
    expect(d.actions).toHaveLength(2);
    expect(d.actions![0].name).toBe("Replace with `rate`");
    expect(d.actions![1].name).toBe("Replace with `irate`");
  });
});

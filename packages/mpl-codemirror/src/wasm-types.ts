/**
 * Shared WASM types and utilities for the MPL language server integration.
 *
 * These types mirror the Rust serde output from `lib/mpl/src/wasm/completions.rs`.
 * Centralised here to avoid duplication across mpl-completions, mpl-hover,
 * and mpl-signature-help.
 */
import * as mpl from "@axiomhq/mpl-lang";

// ── WASM type definitions ──────────────────────────────────────

export type WasmArgType =
  | "Float"
  | { Enum: string[] }
  | { Repeated: { typ: WasmArgType; min: number; max: number | null } }
  | { OneOf: WasmArgType[] }
  | { Optional: WasmArgType };

export interface WasmFunctionArg {
  name: string;
  type: WasmArgType;
}

export interface WasmFunctionInfo {
  label: string;
  args: WasmFunctionArg[];
  info?: string;
}

// ── Formatting helpers ─────────────────────────────────────────

export function formatArgType(typ: WasmArgType): string {
  if (typ === "Float") return "float";
  if (typeof typ === "object") {
    if ("Enum" in typ) return `enum(${typ.Enum.join(", ")})`;
    if ("Repeated" in typ) {
      const { typ: inner, min, max } = typ.Repeated;
      let s = `repeated(${formatArgType(inner)}`;
      if (min > 0) s += `, min=${min}`;
      if (max !== null) s += `, max=${max}`;
      return s + ")";
    }
    if ("OneOf" in typ) return `one_of(${typ.OneOf.map(formatArgType).join(", ")})`;
    if ("Optional" in typ) return `[${formatArgType(typ.Optional)}]`;
  }
  return "unknown";
}

export function formatArgs(args: WasmFunctionArg[]): string | undefined {
  if (args.length === 0) return undefined;
  return `(${args.map(a => `${a.name}: ${formatArgType(a.type)}`).join(", ")})`;
}

// ── function_info WASM lookup with cache ───────────────────────

const fnInfoCache = new Map<string, WasmFunctionInfo | null>();

export function getFunctionInfo(label: string): WasmFunctionInfo | null {
  if (fnInfoCache.has(label)) return fnInfoCache.get(label)!;
  let info: WasmFunctionInfo | null = null;
  try {
    info = mpl.function_info(label) as WasmFunctionInfo | null;
  } catch {
    // WASM not ready
  }
  fnInfoCache.set(label, info);
  return info;
}

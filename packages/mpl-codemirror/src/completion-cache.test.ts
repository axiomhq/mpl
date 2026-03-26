import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { CompletionCache } from "./completion-cache";

describe("CompletionCache", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns undefined on cache miss", () => {
    const cache = new CompletionCache<string[]>();
    expect(cache.get("missing")).toBeUndefined();
  });

  it("returns cached data within TTL", () => {
    const cache = new CompletionCache<string[]>(1000);
    cache.set("key", ["a", "b"]);

    vi.advanceTimersByTime(999);
    expect(cache.get("key")).toEqual(["a", "b"]);
  });

  it("returns undefined after TTL expires", () => {
    const cache = new CompletionCache<string[]>(1000);
    cache.set("key", ["a", "b"]);

    vi.advanceTimersByTime(1001);
    expect(cache.get("key")).toBeUndefined();
  });

  it("treats TTL of 0 as disabled", () => {
    const cache = new CompletionCache<string[]>(0);
    cache.set("key", ["a"]);
    expect(cache.get("key")).toBeUndefined();
  });

  it("treats negative TTL as disabled", () => {
    const cache = new CompletionCache<string[]>(-1);
    cache.set("key", ["a"]);
    expect(cache.get("key")).toBeUndefined();
  });

  it("keeps independent keys separate", () => {
    const cache = new CompletionCache<string[]>(5000);
    cache.set("ds1", ["cpu"]);
    cache.set("ds2", ["mem"]);

    expect(cache.get("ds1")).toEqual(["cpu"]);
    expect(cache.get("ds2")).toEqual(["mem"]);
  });

  it("overwriting a key resets the timestamp", () => {
    const cache = new CompletionCache<string[]>(1000);
    cache.set("key", ["old"]);

    vi.advanceTimersByTime(800);
    cache.set("key", ["new"]);

    vi.advanceTimersByTime(800);
    expect(cache.get("key")).toEqual(["new"]);

    vi.advanceTimersByTime(201);
    expect(cache.get("key")).toBeUndefined();
  });

  it("uses default TTL of 30s when no argument provided", () => {
    const cache = new CompletionCache<string[]>();
    cache.set("key", ["val"]);

    vi.advanceTimersByTime(29_999);
    expect(cache.get("key")).toEqual(["val"]);

    vi.advanceTimersByTime(2);
    expect(cache.get("key")).toBeUndefined();
  });
});

const DEFAULT_TTL_MS = 30_000;

interface CacheEntry<T> {
  data: T;
  timestamp: number;
}

export class CompletionCache<T> {
  private entries = new Map<string, CacheEntry<T>>();
  private ttlMs: number;

  constructor(ttlMs: number = DEFAULT_TTL_MS) {
    this.ttlMs = ttlMs;
  }

  get(key: string): T | undefined {
    if (this.ttlMs <= 0) {
      return undefined;
    }
    const entry = this.entries.get(key);
    if (!entry) {
      return undefined;
    }
    if (Date.now() - entry.timestamp > this.ttlMs) {
      this.entries.delete(key);
      return undefined;
    }
    return entry.data;
  }

  set(key: string, data: T): void {
    if (this.ttlMs <= 0) {
      return;
    }
    this.entries.set(key, { data, timestamp: Date.now() });
  }
}

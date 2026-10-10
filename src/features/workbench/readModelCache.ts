import {isTauri} from '@tauri-apps/api/core';
/** Private presentation snapshots. Never cache commands, approval or auth.
 * One scheduler keeps recently used resources warm across navigation; desktop
 * continues in the background, mobile resumes when visible.
 * A mobile scope is enabled only by a live authenticated session response. */
type Entry = { value?: unknown; at: number; touched: number; due: number; period: number; failed?:boolean; loader?: () => Promise<unknown>; pending?: Promise<unknown>; version: number; listeners: Set<() => void> };
const STORAGE = "agbrio.private-read-cache.v1";
const MAX_AGE = 24 * 60 * 60 * 1000;
const MAX_CHARS = 1_500_000;
export class ReadModelCache {
  private entries = new Map<string, Entry>();
  private scope: string | null = null;
  private epoch = 0;
  private timer?: ReturnType<typeof setInterval>;
  private saveTimer?: ReturnType<typeof setTimeout>;
  private running = 0;
  private wake = () => { if (this.background || document.visibilityState !== "hidden") {for(const e of this.entries.values())if(e.failed||(!this.background&&Date.now()-e.at>=e.period)){e.due=0;e.failed=false;}void this.tick();} };
  constructor(private storage: () => Storage | undefined = () => localStorage, private background=false) {}
  setScope(scope: string | null) {
    if (this.scope === scope) return;
    this.epoch++; this.entries.clear(); this.scope = scope;
    clearTimeout(this.saveTimer);
    try {
      const raw = this.storage()?.getItem(STORAGE);
      const saved = raw && raw.length <= MAX_CHARS ? JSON.parse(raw) : null;
      if (scope && saved?.scope === scope && Array.isArray(saved.rows)) {
        for (const row of saved.rows.slice(0, 48)) if (typeof row.key === "string" && typeof row.at === "number" && row.at <= Date.now() && Date.now() - row.at < MAX_AGE)
          this.entries.set(row.key, {value: row.value, at: row.at, touched: row.at, due: 0, period: 5000, version: 0, listeners: new Set()});
      } else this.storage()?.removeItem(STORAGE);
    } catch { /* unavailable/corrupt storage never blocks the application */ }
  }
  clear() {
    this.epoch++; for (const e of this.entries.values()) { e.version++; e.value = undefined; e.at = 0; e.due = 0; e.listeners.forEach(fn => fn()); }
    this.entries.clear(); clearTimeout(this.saveTimer);
    try { this.storage()?.removeItem(STORAGE); } catch { /* private storage unavailable */ }
  }
  peek<T>(key: string): T | undefined { return this.entries.get(key)?.value as T | undefined; }
  age(key: string) { const at = this.entries.get(key)?.at; return at ? Date.now() - at : Infinity; }
  observedAt(key: string) { return this.entries.get(key)?.at ?? 0; }
  private entry(key: string) {
    let e = this.entries.get(key);
    if (!e) { e = {at: 0, touched: Date.now(), due: 0, period: 5000, version: 0, listeners: new Set()}; this.entries.set(key, e); }
    e.touched = Date.now(); return e;
  }
  private start() {
    if (this.timer) return;
    this.timer = setInterval(() => void this.tick(), 1000);
    document.addEventListener("visibilitychange", this.wake); window.addEventListener("focus", this.wake); window.addEventListener("online", this.wake);
  }
  subscribe(key: string, listener: () => void) { const e = this.entry(key); e.listeners.add(listener); return () => { e.listeners.delete(listener); }; }
  async read<T>(key: string, loader: () => Promise<T>, period = 5000): Promise<T> {
    const e = this.entry(key); e.loader = loader; if(!e.failed&&period<e.period)e.due=Math.min(e.due,e.at+period);e.period = period; this.start();
    if (e.value !== undefined) {
      return e.value as T;
    }
    return this.load(key, e) as Promise<T>;
  }
  refresh<T>(key: string, loader: () => Promise<T>, period = 5000): Promise<T> {
    const e = this.entry(key); e.loader = loader; e.period = period; this.start(); return this.load(key, e) as Promise<T>;
  }
  warm<T>(key:string, loader:()=>Promise<T>, period=30000) {
    const e=this.entry(key); if(!e.loader){e.loader=loader;e.period=period;} this.start();
  }
  put<T>(key: string, value: T) {
    const e = this.entry(key); e.version++; e.value = value; e.at = Date.now(); e.due = e.at + e.period; e.listeners.forEach(fn => fn()); this.save();
  }
  invalidate(prefix = "", discard = false) {
    for (const [key, e] of this.entries) if (key.startsWith(prefix)) {
      e.version++; e.due = 0; e.failed=false;
      if (discard) { e.value = undefined; e.at = 0; }
      e.listeners.forEach(fn => fn());
    }
    this.save();
  }
  retain(prefix:string, ids:Set<string>) {
    for(const [key,e] of this.entries)if(key.startsWith(prefix)&&!ids.has(key.slice(prefix.length))){e.version++;e.loader=undefined;e.value=undefined;e.listeners.forEach(fn=>fn());this.entries.delete(key);}
    this.save();
  }
  private load(key: string, e: Entry): Promise<unknown> {
    if (e.pending) return e.pending;
    if (!e.loader) return Promise.reject(new Error("READ_CACHE_NO_LOADER"));
    const epoch = this.epoch, version = e.version; this.running++;
    const promise = Promise.resolve().then(e.loader).then(value => {
      if (epoch === this.epoch && version === e.version && this.entries.get(key) === e) {
        e.value = value; e.failed=false;e.at = Date.now(); e.due = e.at + e.period; e.listeners.forEach(fn => fn()); this.save();
      } else throw new Error("READ_CACHE_SUPERSEDED");
      return value;
    }).catch(error => { if (epoch === this.epoch && version === e.version){e.failed=true;e.due = Date.now() + Math.max(10000, e.period);} throw error; })
      .finally(() => { if (e.pending === promise) e.pending = undefined; this.running--; });
    e.pending = promise; return promise;
  }
  private async tick() {
    if (!this.background && document.visibilityState === "hidden") return;
    const now = Date.now();
    const due = [...this.entries.entries()].filter(([, e]) => e.loader && !e.pending && e.due <= now && (e.listeners.size || now - e.touched < 20 * 60 * 1000)).sort((a,b) => a[1].due - b[1].due);
    for (const [key, e] of due) { if (this.running >= 2) break; void this.load(key, e).catch(() => undefined); }
    const expired = [...this.entries.entries()].filter(([, e]) => !e.listeners.size && !e.pending).sort((a,b) => b[1].touched - a[1].touched).slice(96);
    for (const [key] of expired) this.entries.delete(key);
  }
  private save() {
    if (!this.scope || this.saveTimer) return;
    this.saveTimer = setTimeout(() => {
      this.saveTimer = undefined;
      const rows: {key:string;at:number;value:unknown}[] = [];
      let size = 0;
      for (const [key, e] of [...this.entries.entries()].sort((a,b) => b[1].touched - a[1].touched)) {
        if (e.value === undefined || Date.now() - e.at >= MAX_AGE || rows.length >= 48) continue;
        // Command receipts, draft/CAS records and model options are never registered here.
        const row = {key, at:e.at, value:e.value}; const length = JSON.stringify(row).length;
        if (size + length > MAX_CHARS - 1000) continue; rows.push(row); size += length;
      }
      try { this.storage()?.setItem(STORAGE, JSON.stringify({scope:this.scope, rows})); } catch { /* quota/private mode: memory cache continues */ }
    }, 250);
  }
  dispose() { clearInterval(this.timer); clearTimeout(this.saveTimer); this.timer = undefined; document.removeEventListener("visibilitychange", this.wake); window.removeEventListener("focus", this.wake); window.removeEventListener("online", this.wake); }
}
export const readModels = new ReadModelCache(()=>localStorage,isTauri());
if (typeof location !== "undefined" && !location.pathname.startsWith("/mobile")) readModels.setScope(`desktop:${location.origin}`);
export const roleCacheKey = (id:string) => `bridge:${id}`;
export const chatCacheKey = (id:string) => `chat:${id}`;
export const historyCacheKey = (id:string) => `history:${id}`;

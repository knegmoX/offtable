// 主线程侧的引擎客户端：Promise 化的 Worker RPC。
import type { Actor, Outcome } from './types';

export class EngineError extends Error {
  constructor(public code: string) {
    super(code);
  }
}

export interface GameState<V> {
  view: V;
  toAct: Actor;
  outcome: Outcome | null;
}

class Engine {
  private worker: Worker | null = null;
  private seq = 0;
  private pending = new Map<number, { resolve: (v: any) => void; reject: (e: unknown) => void }>();

  private ensure(): Worker {
    if (!this.worker) {
      this.worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
      this.worker.onmessage = (e: MessageEvent) => {
        const { id, ok, result, error } = e.data;
        const p = this.pending.get(id);
        if (!p) return;
        this.pending.delete(id);
        ok ? p.resolve(result) : p.reject(new EngineError(error));
      };
    }
    return this.worker;
  }

  call<T>(method: string, ...args: unknown[]): Promise<T> {
    const id = ++this.seq;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ensure().postMessage({ id, method, args });
    });
  }
}

const engine = new Engine();

/// 一个由 Worker 托管的对局句柄。
export class GameHandle<V = unknown, M = unknown> {
  private constructor(
    readonly game: string,
    private h: number,
  ) {}

  static async open<V, M>(game: string, cfg: object, seats: number, seed?: bigint): Promise<GameHandle<V, M>> {
    const s = seed ?? randomSeed();
    const hi = Number((s >> 32n) & 0xffffffffn);
    const lo = Number(s & 0xffffffffn);
    const h = await engine.call<number>('open', game, JSON.stringify(cfg), seats, hi, lo);
    return new GameHandle<V, M>(game, h);
  }

  static async restore<V, M>(game: string, snapshot: string): Promise<GameHandle<V, M>> {
    const h = await engine.call<number>('restore', game, snapshot);
    return new GameHandle<V, M>(game, h);
  }

  state(seat = -1): Promise<GameState<V>> {
    return engine.call('state', this.h, seat);
  }
  views(seats: number[]): Promise<V[]> {
    return engine.call('views', this.h, seats);
  }
  apply(seat: number, mv: M): Promise<unknown[]> {
    return engine.call('apply', this.h, seat, mv);
  }
  bot(seat: number, level: number): Promise<M | null> {
    return engine.call('bot', this.h, seat, level);
  }
  snapshot(): Promise<string> {
    return engine.call('snapshot', this.h);
  }
  close(): Promise<boolean> {
    return engine.call('close', this.h);
  }
}

/// 64 位随机种子（crypto.getRandomValues 在非安全上下文也可用）。
export function randomSeed(): bigint {
  const a = new Uint32Array(2);
  crypto.getRandomValues(a);
  return (BigInt(a[0]) << 32n) | BigInt(a[1]);
}

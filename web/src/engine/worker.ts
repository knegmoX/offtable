// 游戏引擎 Worker：加载 game.wasm，所有规则 / AI 计算都在这里，主线程只负责渲染。
// 局域网主机模式下，这个 Worker 就是房间的权威状态。
import init, { GameSession } from '../wasm/pkg/game.js';

declare const self: {
  postMessage(m: unknown): void;
  onmessage: ((e: MessageEvent) => void) | null;
};

type Req = { id: number; method: string; args: unknown[] };

const sessions = new Map<number, GameSession>();
let nextHandle = 1;
const ready = init();

function get(h: unknown): GameSession {
  const s = sessions.get(h as number);
  if (!s) throw new Error('no_session');
  return s;
}

const methods: Record<string, (...a: any[]) => unknown> = {
  open(game: string, cfg: string, seats: number, seedHi: number, seedLo: number) {
    const s = new GameSession(game, cfg, seats, seedHi >>> 0, seedLo >>> 0);
    const h = nextHandle++;
    sessions.set(h, s);
    return h;
  },
  restore(game: string, snapshot: string) {
    const s = GameSession.restore(game, snapshot);
    const h = nextHandle++;
    sessions.set(h, s);
    return h;
  },
  close(h: number) {
    sessions.get(h)?.free();
    sessions.delete(h);
    return true;
  },
  state(h: number, seat: number) {
    const s = get(h);
    return { view: JSON.parse(s.view(seat)), toAct: JSON.parse(s.toAct()), outcome: JSON.parse(s.outcome()) };
  },
  views(h: number, seats: number[]) {
    const s = get(h);
    return seats.map((seat) => JSON.parse(s.view(seat)));
  },
  legal(h: number, seat: number) {
    return JSON.parse(get(h).legal(seat));
  },
  apply(h: number, seat: number, mv: unknown) {
    return JSON.parse(get(h).apply(seat, JSON.stringify(mv)));
  },
  bot(h: number, seat: number, level: number) {
    const mv = get(h).botMove(seat, level);
    return mv ? JSON.parse(mv) : null;
  },
  snapshot(h: number) {
    return get(h).snapshot();
  },
};

self.onmessage = async (e: MessageEvent) => {
  const { id, method, args } = e.data as Req;
  try {
    await ready;
    const fn = methods[method];
    if (!fn) throw new Error('no_method:' + method);
    self.postMessage({ id, ok: true, result: fn(...args) });
  } catch (err) {
    // wasm 的 JsError 消息就是稳定错误码（occupied / not_your_turn …）
    self.postMessage({ id, ok: false, error: err instanceof Error ? err.message : String(err) });
  }
};

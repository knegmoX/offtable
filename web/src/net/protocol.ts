// 局域网协议（与 crates/protocol 保持一致；JSON，`t` 为类型标签）。
// 访客 ⇄ relay ⇄ 主机。relay 与具体游戏无关：只按令牌路由、缓存最后一帧、限流与鉴权。

export const PROTO = 1;

export type Seat = number;

// ---------- 访客 → relay ----------
export type C2S =
  | { t: 'hello'; proto: number; build: string; key: string; token?: string; name: string }
  | { t: 'intent'; cseq: number; base: number; mv: unknown }
  | { t: 'ping'; ts: number };

// ---------- relay/主机 → 访客 ----------
export interface LobbyPlayer {
  name: string;
  seat: Seat | null;
  online: boolean;
  host?: boolean;
}

export type S2C =
  | { t: 'welcome'; token: string; build: string }
  | { t: 'lobby'; game: string; players: LobbyPlayer[]; started: boolean; you: Seat | null }
  | { t: 'view'; sv: number; view: unknown; you: Seat | null; acked?: number }
  | { t: 'reject'; cseq: number; code: string }
  | { t: 'host'; paused: boolean }
  | { t: 'reload' }
  | { t: 'pong'; ts: number }
  | { t: 'error'; code: 'bad_key' | 'room_full' | 'kicked' | 'bad_proto' | 'rate_limited' | string };

// ---------- relay → 主机（桥：开发期 WebSocket /__bridge，App 内为 Tauri IPC） ----------
export type R2H =
  | { t: 'room'; urls: string[]; key: string; build: string }
  | { t: 'joined'; token: string; name: string; resumed: boolean }
  | { t: 'left'; token: string }
  | { t: 'intent'; token: string; cseq: number; base: number; mv: unknown };

// ---------- 主机 → relay ----------
export type H2R =
  | { t: 'send'; to: string; msg: S2C }
  | { t: 'broadcast'; msg: S2C }
  | { t: 'kick'; token: string };

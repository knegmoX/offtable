// 局域网房主：权威对局在 Worker（game.wasm）里，relay 只负责转发。
// 每次状态变化给每个座位推送一份完整视图（带递增 sv）；访客意图带 base，过期即拒绝。
import { signal } from '@preact/signals';
import { GameHandle, EngineError } from '../engine/client';
import type { LobbyPlayer, R2H, S2C } from '../net/protocol';
import { openBridge, type Bridge } from './bridge';

export interface Player {
  token: string; // 房主自己用 'host'
  name: string;
  seat: number | null;
  online: boolean;
}

export interface RoomInfo {
  urls: string[];
  key: string;
}

export class HostRoom<V = unknown, M = unknown> {
  readonly room = signal<RoomInfo | null>(null);
  readonly players = signal<Player[]>([]);
  readonly started = signal(false);
  readonly view = signal<V | null>(null);
  readonly error = signal<string | null>(null);

  private bridge: Bridge | null = null;
  private handle: GameHandle<V, M> | null = null;
  private sv = 0;
  private busy: Promise<void> = Promise.resolve();

  constructor(
    readonly game: string,
    readonly seats: number,
    readonly cfg: object,
    hostName: string,
    readonly hostSeat = 0,
  ) {
    this.players.value = [{ token: 'host', name: hostName, seat: hostSeat, online: true }];
  }

  async open(): Promise<void> {
    this.bridge = await openBridge((m) => {
      // 串行处理，保证 sv 单调、视图按序下发
      this.busy = this.busy.then(() => this.onRelay(m)).catch((e) => console.error(e));
    });
  }

  close(): void {
    this.bridge?.close();
    this.bridge = null;
    void this.handle?.close();
    this.handle = null;
  }

  private send(to: string, msg: S2C) {
    if (to !== 'host') this.bridge?.send({ t: 'send', to, msg });
  }

  private lobbyFor(p: Player): S2C {
    const players: LobbyPlayer[] = this.players.value.map((q) => ({
      name: q.name,
      seat: q.seat,
      online: q.online,
      host: q.token === 'host',
    }));
    return { t: 'lobby', game: this.game, players, started: this.started.value, you: p.seat };
  }

  private broadcastLobby() {
    for (const p of this.players.value) if (p.online) this.send(p.token, this.lobbyFor(p));
  }

  private freeSeat(): number | null {
    const taken = new Set(this.players.value.map((p) => p.seat).filter((s) => s !== null));
    for (let s = 0; s < this.seats; s++) if (!taken.has(s)) return s;
    return null;
  }

  private async onRelay(m: R2H): Promise<void> {
    switch (m.t) {
      case 'room':
        this.room.value = { urls: m.urls, key: m.key };
        break;
      case 'joined': {
        const list = [...this.players.value];
        let p = list.find((q) => q.token === m.token);
        if (p) {
          p.online = true;
          p.name = m.name || p.name;
        } else {
          p = { token: m.token, name: m.name, seat: this.started.value ? null : this.freeSeat(), online: true };
          list.push(p);
        }
        this.players.value = list;
        this.broadcastLobby();
        if (this.started.value) await this.pushViews();
        break;
      }
      case 'left':
        this.players.value = this.players.value.map((q) => (q.token === m.token ? { ...q, online: false } : q));
        this.broadcastLobby();
        break;
      case 'intent': {
        const p = this.players.value.find((q) => q.token === m.token);
        if (!p || p.seat === null || !this.handle) {
          this.send(m.token, { t: 'reject', cseq: m.cseq, code: 'not_your_turn' });
          return;
        }
        if (m.base !== this.sv) {
          this.send(m.token, { t: 'reject', cseq: m.cseq, code: 'stale' });
          return;
        }
        try {
          await this.handle.apply(p.seat, m.mv as M);
          this.sv++;
          await this.pushViews({ token: m.token, cseq: m.cseq });
        } catch (e) {
          this.send(m.token, { t: 'reject', cseq: m.cseq, code: e instanceof EngineError ? e.code : 'error' });
        }
        break;
      }
    }
  }

  private async pushViews(ack?: { token: string; cseq: number }) {
    if (!this.handle) return;
    const online = this.players.value.filter((p) => p.online);
    const seatsNeeded = online.map((p) => p.seat ?? -1);
    const views = await this.handle.views(seatsNeeded);
    online.forEach((p, i) => {
      if (p.token === 'host') {
        this.view.value = views[i];
      } else {
        this.send(p.token, {
          t: 'view',
          sv: this.sv,
          view: views[i],
          you: p.seat,
          ...(ack && ack.token === p.token ? { acked: ack.cseq } : {}),
        });
      }
    });
  }

  /** 开局（或再来一局）。 */
  async start(): Promise<void> {
    await this.busy;
    if (this.handle) await this.handle.close();
    this.handle = await GameHandle.open<V, M>(this.game, this.cfg, this.seats);
    this.started.value = true;
    this.sv = 1;
    this.broadcastLobby();
    await this.pushViews();
  }

  /** 房主自己落子。 */
  async play(mv: M): Promise<void> {
    const run = async () => {
      if (!this.handle) return;
      try {
        await this.handle.apply(this.hostSeat, mv);
        this.sv++;
        await this.pushViews();
      } catch (e) {
        this.error.value = e instanceof EngineError ? e.code : 'error';
      }
    };
    this.busy = this.busy.then(run);
    await this.busy;
  }

  hostCanPlay(toMove: number): boolean {
    return this.started.value && toMove === this.hostSeat;
  }
}

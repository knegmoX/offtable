// 五子棋界面：人机 / 同屏双人 / 面对面联机（房主）。
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { useSignalEffect } from '@preact/signals';
import { GameHandle, EngineError } from '../../engine/client';
import type { Actor, Outcome } from '../../engine/types';
import { errText, t } from '../../app/i18n';
import { loadJSON, saveJSON } from '../../app/store';
import { bridgeAvailable } from '../../session/bridge';
import { HostRoom } from '../../session/host';
import { Qr } from '../../ui/Qr';
import { GomokuBoard, coordName, type GomokuView, type Pt } from './Board';

type Mode = 'ai' | 'local' | 'lan';

interface Settings {
  mode: Mode;
  level: number;
  humanSeat: number;
}

const SETTINGS_KEY = 'gomoku:settings';
const SAVE_KEY = 'gomoku:save';
const MIN_AI_DELAY_MS = 380; // AI 太快会显得不自然

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

export function GomokuScreen({ onBack }: { onBack: () => void }) {
  const [settings, setSettings] = useState<Settings>(() => ({
    mode: 'ai',
    level: 1,
    humanSeat: 0,
    ...loadJSON<Partial<Settings>>(SETTINGS_KEY, {}),
  }));
  useEffect(() => saveJSON(SETTINGS_KEY, settings), [settings]);

  return (
    <div class="app">
      <header class="topbar">
        <button class="icon-btn" onClick={onBack} aria-label={t('btn.back')}>
          ←
        </button>
        <h1>{t('game.gomoku')}</h1>
        <span style={{ width: 44 }} />
      </header>
      <div class="seg" role="group" aria-label="mode">
        {(['ai', 'local', 'lan'] as Mode[]).map((m) => (
          <button
            key={m}
            aria-pressed={settings.mode === m}
            onClick={() => setSettings({ ...settings, mode: m })}
            data-testid={`mode-${m}`}
          >
            {t(`mode.${m}`)}
          </button>
        ))}
      </div>
      {settings.mode === 'lan' ? (
        <LanHost />
      ) : (
        <LocalGame key={settings.mode} settings={settings} setSettings={setSettings} />
      )}
    </div>
  );
}

// ---------------- 本地对局（人机 / 同屏） ----------------

function LocalGame({ settings, setSettings }: { settings: Settings; setSettings: (s: Settings) => void }) {
  const [view, setView] = useState<GomokuView | null>(null);
  const [toAct, setToAct] = useState<Actor | null>(null);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [thinking, setThinking] = useState(false);
  const [loading, setLoading] = useState(true); // 新局/悔棋初始化期间不接受落子
  const [moving, setMoving] = useState(false); // 一步棋（含 AI 应答）进行中
  const inFlight = useRef(false); // 同步锁：防止渲染前的连点用到过期状态
  const [toast, setToast] = useState<string | null>(null);
  const [announce, setAnnounce] = useState('');
  const handle = useRef<GameHandle<GomokuView, Pt> | null>(null);
  const history = useRef<string[]>([]); // 快照栈（悔棋）
  const gameGen = useRef(0); // 新局/悔棋会作废进行中的异步流程
  const aiGen = useRef(0); // 取消过期的 AI 回调
  const vsAi = settings.mode === 'ai';

  const refresh = async () => {
    const h = handle.current;
    if (!h) return;
    const s = await h.state();
    setView(s.view);
    setToAct(s.toAct);
    setOutcome(s.outcome);
    if (vsAi) saveJSON(SAVE_KEY, { snapshot: await h.snapshot(), level: settings.level, humanSeat: settings.humanSeat });
    return s;
  };

  const maybeAi = async () => {
    const h = handle.current;
    if (!h || !vsAi) return;
    const my = ++aiGen.current;
    const s = await h.state();
    if (s.toAct.t !== 'seat' || s.toAct.seat === settings.humanSeat) return;
    setThinking(true);
    const t0 = performance.now();
    const mv = await h.bot(s.toAct.seat, settings.level);
    const wait = MIN_AI_DELAY_MS - (performance.now() - t0);
    if (wait > 0) await sleep(wait);
    if (my !== aiGen.current || handle.current !== h || !mv) return;
    await h.apply(s.toAct.seat, mv);
    setThinking(false);
    const after = await refresh();
    if (after) speak(after.view, s.toAct.seat);
  };

  const speak = (v: GomokuView, seat: number) => {
    if (!v.last) return;
    setAnnounce(t('a11y.placed', { side: t(seat === 0 ? 'a11y.black' : 'a11y.white'), pos: coordName(v.last, v.size) }));
  };

  const newGame = async (restoreSave = false): Promise<void> => {
    const my = ++gameGen.current;
    aiGen.current++;
    setThinking(false);
    setLoading(true);
    const old = handle.current;
    handle.current = null;
    if (old) await old.close();
    history.current = [];
    const save = restoreSave && vsAi ? loadJSON<{ snapshot?: string } | null>(SAVE_KEY, null) : null;
    const h = save?.snapshot
      ? await GameHandle.restore<GomokuView, Pt>('gomoku', save.snapshot)
      : await GameHandle.open<GomokuView, Pt>('gomoku', { size: 15 }, 2);
    if (my !== gameGen.current) {
      await h.close(); // 期间又开了新局：丢弃这个
      return;
    }
    handle.current = h;
    const s = await refresh();
    setLoading(false);
    if (s?.outcome) {
      // 存档是已结束的对局：直接开新局
      return newGame(false);
    }
    await maybeAi();
  };

  useEffect(() => {
    void newGame(true);
    return () => {
      gameGen.current++;
      aiGen.current++;
      void handle.current?.close();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.mode]);

  const play = async (p: Pt) => {
    const h = handle.current;
    if (!h || inFlight.current) return;
    inFlight.current = true;
    setMoving(true);
    try {
      // 以引擎为准，而不是可能过期的渲染状态
      const cur = await h.state();
      if (cur.toAct.t !== 'seat') return;
      const seat = cur.toAct.seat;
      if (vsAi && seat !== settings.humanSeat) return;
      const snap = await h.snapshot();
      await h.apply(seat, p);
      history.current.push(snap);
      const s = await refresh();
      if (s) speak(s.view, seat);
      await maybeAi();
    } catch (e) {
      setToast(errText(e instanceof EngineError ? e.code : 'generic'));
      setTimeout(() => setToast(null), 1800);
    } finally {
      inFlight.current = false;
      setMoving(false);
    }
  };

  const undo = async () => {
    const snap = history.current.pop();
    if (!snap) return;
    const my = ++gameGen.current;
    aiGen.current++;
    setThinking(false);
    setLoading(true);
    const old = handle.current;
    handle.current = null;
    if (old) await old.close();
    const h = await GameHandle.restore<GomokuView, Pt>('gomoku', snap);
    if (my !== gameGen.current) {
      await h.close();
      return;
    }
    handle.current = h;
    await refresh();
    setLoading(false);
  };

  const status = useMemo(() => {
    if (outcome) {
      if (outcome.t === 'draw') return t('result.draw');
      const w = outcome.seats[0];
      if (vsAi) return w === settings.humanSeat ? t('result.win') : t('result.lose');
      return t(w === 0 ? 'result.0' : 'result.1');
    }
    if (thinking) return t('ai.thinking');
    if (toAct?.t === 'seat') {
      if (vsAi && toAct.seat === settings.humanSeat) return t('turn.you');
      return t(toAct.seat === 0 ? 'turn.0' : 'turn.1');
    }
    return '';
  }, [outcome, thinking, toAct, vsAi, settings.humanSeat]);

  const canPlay =
    !!view &&
    !loading &&
    !moving &&
    !outcome &&
    !thinking &&
    toAct?.t === 'seat' &&
    (!vsAi || toAct.seat === settings.humanSeat);

  return (
    <>
      {vsAi && (
        <>
          <div class="seg" role="group" aria-label="level">
            {[0, 1, 2].map((lv) => (
              <button
                key={lv}
                aria-pressed={settings.level === lv}
                onClick={() => setSettings({ ...settings, level: lv })}
                data-testid={`level-${lv}`}
              >
                {t(`level.${lv}` as 'level.0')}
              </button>
            ))}
          </div>
          <div class="seg" role="group" aria-label="side">
            {[0, 1].map((s) => (
              <button
                key={s}
                aria-pressed={settings.humanSeat === s}
                onClick={() => {
                  setSettings({ ...settings, humanSeat: s });
                  setTimeout(() => void newGame(false), 0);
                }}
              >
                {t(`side.${s}` as 'side.0')}
              </button>
            ))}
          </div>
        </>
      )}
      <div class={`status ${outcome ? 'strong' : ''}`} data-testid="status" aria-live="polite">
        {status}
      </div>
      {view && <GomokuBoard view={view} canPlay={canPlay} busy={loading || thinking || moving} onPlay={play} />}
      <div class="sr-only" aria-live="polite">
        {announce}
      </div>
      <div class="row">
        <button class="btn" onClick={undo} disabled={!history.current.length || thinking} data-testid="undo">
          {t('btn.undo')}
        </button>
        <button class="btn primary" onClick={() => newGame(false)} data-testid="restart">
          {t('btn.restart')}
        </button>
      </div>
      {toast && <div class="toast">{toast}</div>}
    </>
  );
}

// ---------------- 面对面联机：房主 ----------------

function LanHost() {
  const [room, setRoom] = useState<HostRoom<GomokuView, Pt> | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [, force] = useState(0);

  useSignalEffect(() => {
    // 订阅房间信号以触发重绘
    if (room) {
      void room.room.value;
      void room.players.value;
      void room.view.value;
      void room.started.value;
      force((x) => x + 1);
    }
  });

  useEffect(() => () => room?.close(), [room]);

  if (!bridgeAvailable()) {
    return (
      <div class="panel">
        <h2>{t('lan.title')}</h2>
        <p class="muted">{t('lan.needApp')}</p>
      </div>
    );
  }

  const open = async () => {
    const r = new HostRoom<GomokuView, Pt>('gomoku', 2, { size: 15 }, t('lan.host'));
    try {
      await r.open();
      setRoom(r);
    } catch {
      setErr(t('err.generic'));
    }
  };

  if (!room) {
    return (
      <div class="panel">
        <h2>{t('lan.title')}</h2>
        <p class="muted">{t('lan.how')}</p>
        <p class="muted">{t('lan.sameWifi')}</p>
        <button class="btn primary" onClick={open} data-testid="host-open">
          {t('btn.host')}
        </button>
        {err && <p class="muted">{err}</p>}
      </div>
    );
  }

  const info = room.room.value;
  const url = info ? `${info.urls[0]}/?k=${info.key}` : '';
  const view = room.view.value;
  const guests = room.players.value.filter((p) => p.token !== 'host');

  if (room.started.value && view) {
    const out = view.outcome;
    const status = out
      ? out.t === 'draw'
        ? t('result.draw')
        : out.seats[0] === room.hostSeat
          ? t('result.win')
          : t('result.lose')
      : view.to_move === room.hostSeat
        ? t('turn.you')
        : t(view.to_move === 0 ? 'turn.0' : 'turn.1');
    return (
      <>
        <div class={`status ${out ? 'strong' : ''}`} data-testid="status">
          {status}
        </div>
        <GomokuBoard view={view} canPlay={!out && room.hostCanPlay(view.to_move)} onPlay={(p) => void room.play(p)} />
        <div class="row">
          <button class="btn primary" onClick={() => void room.start()} data-testid="host-restart">
            {t('btn.restart')}
          </button>
        </div>
      </>
    );
  }

  return (
    <div class="panel">
      <h2>{t('lan.title')}</h2>
      {info ? (
        <>
          <Qr text={url} label={url} />
          <div class="url" data-testid="room-url">
            {url}
          </div>
          <p class="muted">{t('lan.how')}</p>
        </>
      ) : (
        <p class="muted">…</p>
      )}
      <h2>{t('lan.players')}</h2>
      <ul class="players" data-testid="players">
        {room.players.value.map((p) => (
          <li key={p.token}>
            <span>{p.name}</span>
            <span class="muted">
              {p.online ? (p.seat === null ? t('lan.spectator') : t(`lan.seat.${p.seat}` as 'lan.seat.0')) : t('lan.offline')}
            </span>
          </li>
        ))}
      </ul>
      {!guests.length && <p class="muted">{t('lan.waiting')}</p>}
      <div class="row">
        <button
          class="btn primary"
          disabled={!guests.some((g) => g.online && g.seat !== null)}
          onClick={() => void room.start()}
          data-testid="host-start"
        >
          {t('btn.start')}
        </button>
        <button
          class="btn"
          onClick={() => {
            room.close();
            setRoom(null);
          }}
        >
          {t('btn.close')}
        </button>
      </div>
    </div>
  );
}

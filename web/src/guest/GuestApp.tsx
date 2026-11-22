// 局域网访客页：扫码后在浏览器中打开，免安装。
// 运行在 http://192.168.x.x（非安全上下文）：只用 WebSocket / localStorage / getRandomValues，
// 不用 Service Worker、crypto.subtle、randomUUID、Wake Lock、Web Share、剪贴板。
import { useEffect, useRef, useState } from 'preact/hooks';
import { errText, t } from '../app/i18n';
import { GomokuBoard, type GomokuView, type Pt } from '../games/gomoku/Board';
import { PROTO, type C2S, type LobbyPlayer, type S2C } from '../net/protocol';


type Phase = 'connecting' | 'lobby' | 'playing' | 'reconnecting' | 'fatal';

function randomName(): string {
  const a = new Uint8Array(1);
  crypto.getRandomValues(a);
  return `${t('guest.defaultName')} ${10 + (a[0] % 90)}`;
}

function store(k: string, v?: string): string | null {
  try {
    if (v !== undefined) localStorage.setItem('offtable:' + k, v);
    return localStorage.getItem('offtable:' + k);
  } catch {
    return null;
  }
}

export function GuestApp() {
  const params = new URLSearchParams(location.search);
  const key = params.get('k') || '';
  const [phase, setPhase] = useState<Phase>('connecting');
  const [fatal, setFatal] = useState<string>('');
  const [players, setPlayers] = useState<LobbyPlayer[]>([]);
  const [you, setYou] = useState<number | null>(null);
  const [view, setView] = useState<GomokuView | null>(null);
  const [paused, setPaused] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const ws = useRef<WebSocket | null>(null);
  const sv = useRef(0);
  const cseq = useRef(0);
  const retry = useRef(0);
  const stopped = useRef(false);

  const name = store('name') || store('name', randomName()) || randomName();

  const send = (m: C2S) => {
    if (ws.current?.readyState === WebSocket.OPEN) ws.current.send(JSON.stringify(m));
  };

  const flash = (s: string) => {
    setToast(s);
    setTimeout(() => setToast(null), 1800);
  };

  useEffect(() => {
    if (!key) {
      setFatal(errText('bad_key'));
      setPhase('fatal');
      return;
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    const connect = () => {
      const sock = new WebSocket(`ws://${location.host}/ws`);
      ws.current = sock;
      sock.onopen = () => {
        retry.current = 0;
        const token = store(`token:${key}`) || undefined;
        send({ t: 'hello', proto: PROTO, build: __BUILD__, key, token, name });
      };
      sock.onmessage = (e) => {
        const m = JSON.parse(e.data) as S2C;
        switch (m.t) {
          case 'welcome':
            store(`token:${key}`, m.token);
            setPhase((p) => (p === 'playing' ? p : 'lobby'));
            break;
          case 'lobby':
            setPlayers(m.players);
            setYou(m.you);
            if (!m.started) setPhase('lobby');
            break;
          case 'view':
            sv.current = m.sv;
            setYou(m.you);
            setView(m.view as GomokuView);
            setPaused(false);
            setPhase('playing');
            break;
          case 'reject':
            flash(errText(m.code));
            break;
          case 'host':
            setPaused(m.paused);
            break;
          case 'reload':
            location.reload();
            break;
          case 'error':
            if (m.code === 'rate_limited') {
              flash(errText(m.code));
            } else {
              stopped.current = true;
              setFatal(errText(m.code));
              setPhase('fatal');
            }
            break;
        }
      };
      sock.onclose = () => {
        if (stopped.current) return;
        setPhase((p) => (p === 'fatal' ? p : 'reconnecting'));
        const delay = Math.min(8000, 1000 * 2 ** retry.current++);
        timer = setTimeout(connect, delay);
      };
    };
    connect();
    const ping = setInterval(() => send({ t: 'ping', ts: Date.now() }), 15000);
    return () => {
      stopped.current = true;
      clearTimeout(timer);
      clearInterval(ping);
      ws.current?.close();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const play = (p: Pt) => send({ t: 'intent', cseq: ++cseq.current, base: sv.current, mv: p });

  const seatName = (s: number | null) => (s === null ? t('lan.spectator') : t(`lan.seat.${s}` as 'lan.seat.0'));

  let status = '';
  if (phase === 'connecting') status = t('guest.connecting');
  else if (phase === 'reconnecting') status = t('guest.reconnecting');
  else if (phase === 'lobby') status = t('guest.waiting');
  else if (phase === 'playing' && view) {
    const out = view.outcome;
    if (out) {
      status = out.t === 'draw' ? t('result.draw') : you !== null && out.seats.includes(you) ? t('result.win') : you === null ? t(out.seats[0] === 0 ? 'result.0' : 'result.1') : t('result.lose');
    } else if (you !== null && view.to_move === you) status = t('turn.you');
    else status = t(view.to_move === 0 ? 'turn.0' : 'turn.1');
  }

  return (
    <div class="app">
      <header class="topbar">
        <h1>{t('game.gomoku')}</h1>
      </header>
      {phase === 'fatal' ? (
        <div class="panel">
          <p data-testid="guest-fatal">{fatal}</p>
        </div>
      ) : (
        <>
          <div class="muted" style={{ textAlign: 'center' }} data-testid="guest-seat">
            {you === null ? t('guest.spectating') : `${t('guest.youAre')} ${seatName(you)}`} · {name}
          </div>
          <div class={`status ${view?.outcome ? 'strong' : ''}`} data-testid="guest-status" aria-live="polite">
            {status}
          </div>
          {paused && <div class="overlay-note">{t('guest.paused')}</div>}
          {view && (
            <GomokuBoard
              view={view}
              canPlay={phase === 'playing' && !paused && !view.outcome && you !== null && view.to_move === you}
              onPlay={play}
            />
          )}
          {phase === 'lobby' && (
            <ul class="players" data-testid="guest-players">
              {players.map((p, i) => (
                <li key={i}>
                  <span>{p.host ? `${p.name} (${t('lan.host')})` : p.name}</span>
                  <span class="muted">{p.online ? seatName(p.seat) : t('lan.offline')}</span>
                </li>
              ))}
            </ul>
          )}
          {view?.outcome && <p class="muted" style={{ textAlign: 'center' }}>{t('guest.liked')}</p>}
        </>
      )}
      {toast && <div class="toast">{toast}</div>}
    </div>
  );
}

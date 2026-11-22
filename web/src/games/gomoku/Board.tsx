// 五子棋棋盘（SVG）。App、PWA 与局域网访客页共用；只在数据变化时重绘，无 rAF 循环。
import { useRef, useState } from 'preact/hooks';
import { t } from '../../app/i18n';

export interface Pt {
  x: number;
  y: number;
}

export interface GomokuView {
  size: number;
  board: string; // '.' 空 / 'x' 黑 / 'o' 白，行优先
  to_move: number;
  last: Pt | null;
  moves: number;
  outcome: { t: 'win'; seats: number[] } | { t: 'draw' } | null;
  win_line: [Pt, Pt] | null;
  exact_five: boolean;
}

const CELL = 10;

function starPoints(n: number): Pt[] {
  if (n < 13) return [{ x: (n - 1) / 2, y: (n - 1) / 2 }];
  const a = 3;
  const b = n - 4;
  const m = (n - 1) / 2;
  return [
    { x: a, y: a },
    { x: b, y: a },
    { x: m, y: m },
    { x: a, y: b },
    { x: b, y: b },
  ];
}

export function coordName(p: Pt, n: number): string {
  return `${String.fromCharCode(65 + p.x)}${n - p.y}`;
}

interface Props {
  view: GomokuView;
  canPlay: boolean;
  /** 加载中 / AI 思考中（无障碍与自动化测试据此等待）。 */
  busy?: boolean;
  onPlay: (p: Pt) => void;
}

export function GomokuBoard({ view, canPlay, busy = false, onPlay }: Props) {
  const n = view.size;
  const span = (n - 1) * CELL;
  const pad = CELL;
  const total = span + pad * 2;
  const svgRef = useRef<SVGSVGElement>(null);
  const [cursor, setCursor] = useState<Pt>({ x: (n - 1) >> 1, y: (n - 1) >> 1 });
  const [focused, setFocused] = useState(false);

  const cellAt = (p: Pt) => view.board[p.y * n + p.x];

  const tryPlay = (p: Pt) => {
    if (!canPlay || p.x < 0 || p.y < 0 || p.x >= n || p.y >= n) return;
    if (cellAt(p) !== '.') return;
    onPlay(p);
  };

  const onPointer = (e: PointerEvent) => {
    const svg = svgRef.current;
    if (!svg) return;
    const ctm = svg.getScreenCTM();
    if (!ctm) return;
    const pt = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
    const p = { x: Math.round((pt.x - pad) / CELL), y: Math.round((pt.y - pad) / CELL) };
    setCursor(p);
    tryPlay(p);
  };

  const onKey = (e: KeyboardEvent) => {
    const d: Record<string, [number, number]> = {
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
    };
    if (d[e.key]) {
      e.preventDefault();
      const [dx, dy] = d[e.key];
      setCursor((c) => ({ x: Math.min(n - 1, Math.max(0, c.x + dx)), y: Math.min(n - 1, Math.max(0, c.y + dy)) }));
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      tryPlay(cursor);
    }
  };

  const lines = [];
  for (let i = 0; i < n; i++) {
    const o = pad + i * CELL;
    lines.push(<line key={`h${i}`} x1={pad} y1={o} x2={pad + span} y2={o} />);
    lines.push(<line key={`v${i}`} x1={o} y1={pad} x2={o} y2={pad + span} />);
  }

  const stones = [];
  for (let i = 0; i < view.board.length; i++) {
    const c = view.board[i];
    if (c === '.') continue;
    const x = pad + (i % n) * CELL;
    const y = pad + Math.floor(i / n) * CELL;
    stones.push(
      <circle
        key={i}
        cx={x}
        cy={y}
        r={CELL * 0.44}
        fill={c === 'x' ? 'var(--stone-black)' : 'var(--stone-white)'}
        stroke={c === 'x' ? '#000' : '#9a948a'}
        stroke-width={0.35}
      />,
    );
  }

  const cursorCell = cellAt(cursor);
  const cursorLabel = `${coordName(cursor, n)} ${
    cursorCell === 'x' ? t('a11y.black') : cursorCell === 'o' ? t('a11y.white') : t('a11y.empty')
  }`;

  return (
    <div class="board-wrap">
      <svg
        ref={svgRef}
        viewBox={`0 0 ${total} ${total}`}
        role="application"
        aria-label={`${t('a11y.board')} ${n}×${n}. ${cursorLabel}`}
        tabIndex={0}
        onPointerUp={onPointer}
        onKeyDown={onKey}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        data-testid="gomoku-board"
        data-moves={view.moves}
        aria-busy={busy}
      >
        <g stroke="var(--board-line)" stroke-width={0.4} stroke-linecap="square">
          {lines}
        </g>
        {starPoints(n).map((p) => (
          <circle key={`s${p.x}-${p.y}`} cx={pad + p.x * CELL} cy={pad + p.y * CELL} r={1} fill="var(--board-line)" />
        ))}
        {stones}
        {view.last && (
          <circle cx={pad + view.last.x * CELL} cy={pad + view.last.y * CELL} r={1.3} fill="var(--last)" />
        )}
        {view.win_line && (
          <line
            x1={pad + view.win_line[0].x * CELL}
            y1={pad + view.win_line[0].y * CELL}
            x2={pad + view.win_line[1].x * CELL}
            y2={pad + view.win_line[1].y * CELL}
            stroke="var(--last)"
            stroke-width={1.2}
            stroke-linecap="round"
            opacity={0.85}
          />
        )}
        {focused && (
          <rect
            x={pad + cursor.x * CELL - CELL / 2}
            y={pad + cursor.y * CELL - CELL / 2}
            width={CELL}
            height={CELL}
            fill="none"
            stroke="var(--accent)"
            stroke-width={0.8}
            rx={1.5}
          />
        )}
      </svg>
    </div>
  );
}

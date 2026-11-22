import { useState } from 'preact/hooks';
import { t, type Key } from './i18n';
import { GomokuScreen } from '../games/gomoku/GomokuScreen';

const GAMES: { id: string; key: Key; ready: boolean }[] = [
  { id: 'gomoku', key: 'game.gomoku', ready: true },
  { id: 'xiangqi', key: 'game.xiangqi', ready: false },
  { id: 'reversi', key: 'game.reversi', ready: false },
  { id: 'ludo', key: 'game.ludo', ready: false },
  { id: 'sudoku', key: 'game.sudoku', ready: false },
  { id: 'minesweeper', key: 'game.minesweeper', ready: false },
  { id: 'blocks', key: 'game.blocks', ready: false },
  { id: 'undercover', key: 'game.undercover', ready: false },
];

function initialRoute(): string | null {
  const g = new URLSearchParams(location.search).get('game');
  return g && GAMES.some((x) => x.id === g && x.ready) ? g : null;
}

export function App() {
  const [route, setRoute] = useState<string | null>(initialRoute);

  if (route === 'gomoku') return <GomokuScreen onBack={() => setRoute(null)} />;

  return (
    <div class="app">
      <div class="hero">
        <div class="brand">{t('app.name')}</div>
        <div class="tagline">{t('app.tagline')}</div>
      </div>
      <div class="grid">
        {GAMES.map((g) => (
          <button
            key={g.id}
            class="card"
            aria-disabled={!g.ready}
            onClick={() => g.ready && setRoute(g.id)}
            data-testid={`game-${g.id}`}
          >
            <span class="name">{t(g.key)}</span>
            <span class="sub">{g.ready ? t('home.modes') : t('home.soon')}</span>
          </button>
        ))}
      </div>
      <div class="footer">{t('footer.promise')}</div>
    </div>
  );
}

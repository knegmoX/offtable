import { useMemo } from 'preact/hooks';
import { renderSVG } from 'uqr';

/** 二维码（纯本地生成，不依赖网络）。 */
export function Qr({ text, label }: { text: string; label: string }) {
  const svg = useMemo(() => renderSVG(text, { border: 1 }), [text]);
  return <div class="qr" role="img" aria-label={label} dangerouslySetInnerHTML={{ __html: svg }} />;
}

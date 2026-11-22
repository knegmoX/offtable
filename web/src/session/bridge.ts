// 主机 ⇄ relay 的桥。
// - App 内：Tauri IPC（relay 由原生层启动，消息经 Channel 回传）。
// - 开发 / 自动化测试：页面由本机 relay 提供，URL 带 ?bridge=<secret>，走 ws://127.0.0.1/__bridge。
// - 纯 PWA：没有桥（浏览器不能监听端口），开房按钮提示去 App。
import type { H2R, R2H } from '../net/protocol';

export interface Bridge {
  send(msg: H2R): void;
  close(): void;
}

type Listener = (m: R2H) => void;

function inTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

export function bridgeAvailable(): boolean {
  return inTauri() || new URLSearchParams(location.search).has('bridge');
}

export async function openBridge(onMessage: Listener): Promise<Bridge> {
  if (inTauri()) {
    // 按需加载（PWA 与访客页不会打包进来）
    const { invoke, Channel } = await import('@tauri-apps/api/core');
    const ch = new Channel<string>();
    ch.onmessage = (line) => onMessage(JSON.parse(line) as R2H);
    const info = await invoke<{ urls: string[]; key: string }>('relay_start', { onEvent: ch });
    // relay 也会通过 R2H room 推送同样的信息；这里直接回填，避免竞态
    onMessage({ t: 'room', urls: info.urls, key: info.key, build: '' });
    return {
      send: (msg) => void invoke('relay_send', { msg: JSON.stringify(msg) }),
      close: () => void invoke('relay_stop'),
    };
  }
  const secret = new URLSearchParams(location.search).get('bridge');
  if (!secret) throw new Error('no_bridge');
  const ws = new WebSocket(`ws://${location.host}/__bridge?s=${encodeURIComponent(secret)}`);
  ws.onmessage = (e) => onMessage(JSON.parse(e.data) as R2H);
  await new Promise<void>((resolve, reject) => {
    ws.onopen = () => resolve();
    ws.onerror = () => reject(new Error('bridge_failed'));
  });
  return {
    send: (msg) => ws.send(JSON.stringify(msg)),
    close: () => ws.close(),
  };
}

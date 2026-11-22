// 本地存储（Web：localStorage；App 内后续换成 Rust 原子写文件以享受系统备份）。
const PREFIX = 'offtable:';

export function loadJSON<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

export function saveJSON(key: string, value: unknown): void {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    /* 隐私模式 / 配额满：静默失败，不影响游戏 */
  }
}

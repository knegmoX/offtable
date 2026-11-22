// 启动本机 relay（dev-bridge），解析其首行 JSON。
import { spawn, type ChildProcess } from 'node:child_process';
import { createInterface } from 'node:readline';
import path from 'node:path';

export interface RelayInfo {
  port: number;
  key: string;
  secret: string;
  build: string;
  urls: string[];
  host_url: string;
  proc: ChildProcess;
}

const ROOT = path.resolve(import.meta.dirname, '../..');
const BIN =
  process.env.RELAY_BIN ||
  path.join(process.env.CARGO_TARGET_DIR || path.join(ROOT, 'target'), 'debug', 'offtable-relay');

export function startRelay(): Promise<RelayInfo> {
  const proc = spawn(BIN, ['--static', path.join(ROOT, 'web/dist'), '--port', '0'], { stdio: ['ignore', 'pipe', 'inherit'] });
  return new Promise((resolve, reject) => {
    const rl = createInterface({ input: proc.stdout! });
    rl.once('line', (line) => resolve({ ...JSON.parse(line), proc }));
    proc.once('exit', (code) => reject(new Error(`relay exited ${code}`)));
  });
}

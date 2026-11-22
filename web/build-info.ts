import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')) as { version?: string };
let rev = 'dev';
try {
  rev = execSync('git rev-parse --short HEAD', { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim() || 'dev';
} catch {
  /* 非 git 环境 */
}
/** 主机与访客页共用的构建号；relay 用它拒绝版本不一致的访客。 */
export const build = process.env.OFFTABLE_BUILD || `${pkg.version ?? '0.0.0'}+${rev}`;

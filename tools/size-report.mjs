// node tools/size-report.mjs [web/dist] —— Web 产物体积（brotli-11）与预算门禁，输出 Markdown。
import { readdirSync, readFileSync, statSync, appendFileSync } from 'node:fs';
import { join } from 'node:path';
import { brotliCompressSync, constants } from 'node:zlib';

const dist = process.argv[2] || 'web/dist';
const br = (buf) => brotliCompressSync(buf, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length;
const files = (dir) => readdirSync(dir).map((f) => join(dir, f)).filter((f) => statSync(f).isFile());

// 预算（brotli 后字节），见 docs：shell ≤50K、wasm ≤120K、访客页 ≤100K
const groups = [
  { name: '外壳 JS（入口 + 懒加载）', match: (f) => /\/assets\/(index|core)-.*\.js$/.test(f), budget: 50_000 },
  { name: '引擎 Worker JS', match: (f) => /\/assets\/worker-.*\.js$/.test(f), budget: 15_000 },
  { name: 'game.wasm', match: (f) => /\.wasm$/.test(f), budget: 120_000 },
  { name: '访客页 JS+CSS', match: (f) => /\/g\/.*\.(js|css)$/.test(f), budget: 100_000 },
];
const all = [...files(join(dist, 'assets')), ...files(join(dist, 'g'))];
let failed = false;
const rows = groups.map((g) => {
  const hit = all.filter(g.match);
  const raw = hit.reduce((s, f) => s + statSync(f).size, 0);
  const b = hit.reduce((s, f) => s + br(readFileSync(f)), 0);
  const ok = b <= g.budget;
  if (!ok) failed = true;
  return `| ${g.name} | ${hit.length} | ${(raw / 1024).toFixed(1)} KB | ${(b / 1024).toFixed(1)} KB | ${(g.budget / 1024).toFixed(0)} KB | ${ok ? '✅' : '❌'} |`;
});
const md = ['### Web 体积（brotli-11）', '', '| 分组 | 文件数 | 原始 | brotli | 预算 | |', '|---|---|---|---|---|---|', ...rows, ''].join('\n');
console.log(md);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, md + '\n');
if (failed) {
  console.error('体积超出预算');
  process.exit(1);
}

// node scripts/brsize.mjs <file...> —— 打印原始与 brotli(11) 压缩后体积
import { readFileSync } from 'node:fs';
import { brotliCompressSync, constants } from 'node:zlib';
for (const f of process.argv.slice(2)) {
  const buf = readFileSync(f);
  const br = brotliCompressSync(buf, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } });
  console.log(`${f}: ${buf.length} raw, ${br.length} br`);
}

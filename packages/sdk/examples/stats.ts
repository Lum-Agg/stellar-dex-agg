/**
 * Fetch public on-chain stats (analytics-indexer rollup).
 *
 * Run:
 *   npx tsx packages/sdk/examples/stats.ts
 *   FORMAT=csv npx tsx packages/sdk/examples/stats.ts
 */

import { LumAggClient } from '../src/index';

const API_URL = process.env.API_URL || 'https://api.lumagg.xyz';
const DAY = process.env.DAY;
const FORMAT = process.env.FORMAT === 'csv' ? 'csv' : 'json';

async function main() {
  const client = new LumAggClient({ apiUrl: API_URL });

  if (FORMAT === 'csv') {
    const csv = await client.getStats({ day: DAY, format: 'csv' });
    console.log(typeof csv === 'string' ? csv : '');
    return;
  }

  const stats = await client.getStats({ day: DAY });
  if (typeof stats === 'string') return;

  console.log('Stats OK');
  console.log('  invocations:', stats.invocationCount);
  console.log('  cursor_ledger:', stats.cursorLedger);
  console.log('  days:', stats.daily.length);
  for (const d of stats.daily.slice(-3)) {
    console.log(`  ${d.day}: tx=${d.txCount} users=${d.uniqueUsers} split=${d.splitSwapCount}`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

import { test, expect } from '@playwright/test';
import { NATIVE_CONTRACT, USDC_CONTRACT } from '../src/lib/tokenDisplay';

test('headlines user swaps separately from arbitrage and labels combined volume', async ({
  page,
}) => {
  await page.route('**/*', async (route) => {
    const url = new URL(route.request().url());
    if (url.hostname === '127.0.0.1') return route.continue();
    if (url.pathname === '/api/v1/stats') {
      return route.fulfill({
        json: {
          success: true,
          data: {
            invocation_count: 105,
            cursor_ledger: 1,
            oldest_created_at: null,
            daily: [
              {
                day: '2026-10-28',
                tx_count: 100,
                success_count: 100,
                failed_count: 5,
                unique_users: 4,
                total_amount_in: 100,
                split_swap_count: 1,
                by_function: { swap: 2, round_trip_swap: 97, unknown: 1 },
                by_dex: {},
                // Only one surplus record, but 97 successful arbitrage calls.
                round_trip_count: 1,
                total_amount_in_usd: 10,
                total_routed_dex_volume_usd: 30,
              },
            ],
          },
        },
      });
    }
    return route.fulfill({ json: { success: true, data: [] } });
  });
  await page.goto('/stats');
  const breakdown = page.getByRole('region', { name: 'User swap and arbitrage breakdown' });
  await expect(breakdown).toContainText('User swaps');
  await expect(breakdown).toContainText('2.00% of successful transactions');
  await expect(breakdown).toContainText('97.00% of successful transactions');
  await expect(breakdown).toContainText('Arbitrage is not user adoption');
  await expect(
    page.getByRole('heading', { name: 'Daily combined volume (includes arbitrage)' }),
  ).toBeVisible();
  const mix = page.getByRole('region', { name: 'User swaps vs arbitrage over time', exact: true });
  await expect(mix.getByText('Other', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('region', { name: 'Daily rollup table' })).toBeVisible();
  await expect(page.getByRole('columnheader', { name: 'Other', exact: true })).toHaveCount(0);
  const row = page.getByRole('row').filter({ hasText: '2026-10-28' });
  await expect(row.getByRole('cell').nth(1)).toHaveText('100');
  await expect(row.getByRole('cell').nth(2)).toHaveText('2');
  await expect(row.getByRole('cell').nth(3)).toHaveText('97');
});

test('unavailable USD amounts show zero in Daily rollup but not in summary metrics', async ({ page }) => {
  await page.route('**/*', async (route) => {
    const url = new URL(route.request().url());
    if (url.hostname === '127.0.0.1') return route.continue();
    return route.fulfill({
      json: {
        success: true,
        data:
          url.pathname === '/api/v1/stats'
            ? {
                invocation_count: 1,
                cursor_ledger: 1,
                oldest_created_at: null,
                daily: [
                  {
                    day: '2026-10-29',
                    tx_count: 1,
                    success_count: 1,
                    failed_count: 0,
                    unique_users: 1,
                    total_amount_in: 100,
                    split_swap_count: 0,
                    by_function: { round_trip_swap: 1 },
                    by_dex: { aquarius: 4 },
                    round_trip_count: 1,
                    total_amount_in_usd: 10,
                    routed_leg_count: 0,
                    routed_priced_leg_count: 0,
                  },
                ],
              }
            : [],
      },
    });
  });
  await page.goto('/stats');
  const card = page.getByText('Combined routed (priced USD)', { exact: true }).locator('..');
  await expect(card).toContainText('—');
  await expect(card).toContainText('0% legs priced');
  await expect(card).not.toContainText('$0');
  await expect(page.getByRole('region', { name: 'Daily rollup table' })).toBeVisible();
  const row = page.getByRole('row').filter({ hasText: '2026-10-29' });
  const cells = row.getByRole('cell');
  await expect(cells.nth(5)).toHaveText('$10');
  await expect(cells.nth(6)).toHaveText('$0');
  await expect(cells.nth(7)).toHaveText('$0');
  await expect(cells.nth(8)).toHaveText('—');
});

test('chart lines stay solid through zero-filled dates and amounts', async ({
  page,
}) => {
  const daily = Array.from({ length: 7 }, (_, index) => {
    const failedOnly = index === 1;
    const usd = failedOnly || index === 3 ? null : 10 + index;
    return {
      day: `2026-11-0${index + 1}`,
      tx_count: failedOnly ? 0 : 1,
      success_count: failedOnly ? 0 : 1,
      failed_count: failedOnly ? 12 : 0,
      unique_users: failedOnly ? 0 : 1,
      total_amount_in: 0,
      split_swap_count: 0,
      by_function: failedOnly ? {} : { round_trip_swap: 1 },
      by_dex: failedOnly ? {} : { aquarius: 2 },
      total_amount_in_usd: usd,
      total_routed_dex_volume_usd: usd,
      round_trip_gross_surplus_usd: usd,
      round_trip_by_token: usd === null ? [] : [
        index < 3
          ? { base_token: NATIVE_CONTRACT, tx_count: 1, gross_surplus: 1250000 }
          : { base_token: USDC_CONTRACT, tx_count: 1, gross_surplus: '250000' },
      ],
    };
  }).filter((_, index) => index !== 4);
  await page.route('**/*', async (route) => {
    const url = new URL(route.request().url());
    if (url.hostname === '127.0.0.1') return route.continue();
    return route.fulfill({
      json: {
        success: true,
        data:
          url.pathname === '/api/v1/stats'
            ? { invocation_count: 17, cursor_ledger: 1, oldest_created_at: null, daily }
            : [],
      },
    });
  });
  await page.goto('/stats');
  const continuous = /^M[^ML]*(?:L[^ML]*){6}$/;
  for (const [title, selector, count] of [
    ['Daily combined volume (includes arbitrage)', 'path.recharts-area-curve', 2],
    ['Arbitrage gross surplus', 'path.recharts-area-curve', 1],
    ['Caller activity', 'path.recharts-line-curve', 1],
  ] as const) {
    const chart = page.getByRole('region', { name: `${title} chart`, exact: true });
    const lines = chart.locator(selector);
    await expect(lines).toHaveCount(count);
    for (const line of await lines.all()) {
      await expect(line).toHaveAttribute('d', continuous);
      await expect(line).not.toHaveAttribute('stroke-dasharray', /\S/);
    }
  }
  const native = page.getByRole('region', { name: 'Arbitrage gross surplus by token', exact: true });
  for (const asset of ['XLM', 'USDC']) {
    const chart = native.getByRole('region', { name: `Arbitrage gross surplus in ${asset} chart` });
    const surface = chart.locator('svg.recharts-surface');
    await expect(surface).toBeVisible();
    await expect(chart.locator('path.recharts-area-curve')).toHaveAttribute('d', continuous);
    await surface.focus();
    await surface.press('ArrowRight');
    await expect(chart.locator('[data-chart-tooltip]')).toContainText(new RegExp(`0(?:\\.\\d+)? ${asset}`));
  }
  await expect(page.getByRole('region', { name: 'Daily rollup table' })).toBeVisible();
  const failedOnlyDay = page.getByRole('row').filter({ hasText: '2026-11-02' });
  for (const column of [5, 6, 7]) {
    await expect(failedOnlyDay.getByRole('cell').nth(column)).toHaveText('$0');
  }
});

for (const theme of ['light', 'dark']) {
  test(`library charts render in ${theme} mode with shared range and keyboard tooltips`, async ({
    page,
  }, testInfo) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.addInitScript((theme) => localStorage.setItem('lumagg-theme', theme), theme);
    const daily = Array.from({ length: 45 }, (_, index) => {
      const day = new Date(Date.UTC(2026, 8, 15 + index)).toISOString().slice(0, 10);
      const arbitrage = 8 + ((index * 17) % 70);
      const swaps = 1 + (index % 8);
      const notional = 120 + ((index * 431) % 2100);
      return {
        day,
        tx_count: arbitrage + swaps,
        success_count: arbitrage + swaps,
        failed_count: (index * 7) % 30,
        unique_users: 4 + (index % 6),
        total_amount_in: 0,
        split_swap_count: 0,
        by_function: { swap: swaps, round_trip_swap: arbitrage },
        by_dex: { aquarius: 240 },
        round_trip_count: arbitrage,
        total_amount_in_usd: notional,
        total_routed_dex_volume_usd: index === 30 ? null : notional * 2.7,
        round_trip_gross_surplus_usd: index === 30 ? null : ((index * 13) % 170) / 10,
        routed_priced_leg_count: 200,
      };
    });
    await page.route('**/*', async (route) => {
      const url = new URL(route.request().url());
      if (url.hostname === '127.0.0.1') return route.continue();
      return route.fulfill({
        json: {
          success: true,
          data:
            url.pathname === '/api/v1/stats'
              ? { invocation_count: 3000, cursor_ledger: 1, oldest_created_at: null, daily }
              : [],
        },
      });
    });
    await page.goto('/stats');
    const rollup = page.getByRole('region', { name: 'Daily rollup table' });
    await expect(rollup).toBeVisible();
    expect(await rollup.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
    await rollup.evaluate((element) => { element.scrollTop = 100; });
    expect(await rollup.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    await expect(page.getByText('30 indexed days', { exact: false })).toBeVisible();
    const range = page.getByRole('group', { name: 'Chart date range' });
    await range.getByRole('button', { name: 'All', exact: true }).click();
    await expect(page.getByText('45 indexed days', { exact: false })).toBeVisible();
    const mix = page.getByRole('region', {
      name: 'User swaps vs arbitrage over time chart',
      exact: true,
    });
    await mix.scrollIntoViewIfNeeded();
    const surface = mix.locator('svg.recharts-surface');
    await expect(surface).toBeVisible();
    await surface.focus();
    await surface.press('ArrowRight');
    await expect(mix.locator('[data-chart-tooltip]')).toContainText('Arbitrage');
    await expect(mix.locator('[data-chart-tooltip]')).toContainText('User swaps');
    await expect(mix.locator('[data-chart-tooltip]')).not.toContainText('Other');
    await range.getByRole('button', { name: '14d', exact: true }).click();
    await expect(page.getByText('14 indexed days', { exact: false })).toBeVisible();
    await expect(range.getByRole('button', { name: '14d', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    for (const title of [
      'Daily combined volume (includes arbitrage)',
      'Execution outcome',
      'Caller activity',
      'Arbitrage gross surplus',
    ]) {
      const chart = page.getByRole('region', { name: `${title} chart`, exact: true });
      await expect(chart.locator('svg.recharts-surface')).toBeVisible();
    }
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    await range.getByRole('button', { name: 'All', exact: true }).click();
    // Recharts uses JS-driven transitions; wait for the default 1.5s animation.
    await page.waitForTimeout(1800);
    await page
      .getByRole('region', { name: 'User swaps vs arbitrage over time', exact: true })
      .screenshot({ path: testInfo.outputPath(`activity-${theme}.png`), animations: 'disabled' });
    await page
      .getByRole('region', { name: 'Daily combined volume (includes arbitrage)', exact: true })
      .screenshot({ path: testInfo.outputPath(`volume-${theme}.png`), animations: 'disabled' });
    expect(errors).toEqual([]);
  });
}

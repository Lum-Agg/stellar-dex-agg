import { test, expect, XLM, USDC, CUSTOM, ADDRESS, HASH } from './fixtures';

async function readySwap(page: import('@playwright/test').Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Wallet menu' })).toBeVisible();
  await page.getByRole('textbox', { name: 'Amount of XLM to sell' }).fill('10');
  await expect(page.getByRole('button', { name: 'Review & swap', exact: true })).toBeEnabled();
}

test('shared URL restores an async catalog token and switching pairs updates the URL', async ({
  page,
}) => {
  await page.goto(`/?token_in=${CUSTOM}&token_out=${USDC}`);
  await expect(page.getByRole('textbox', { name: 'Amount of TEST to sell' })).toBeVisible();
  await page.getByRole('button', { name: 'Swap sell and buy tokens' }).click();
  await expect(page.getByRole('textbox', { name: 'Amount of USDC to sell' })).toBeVisible();
  await expect(page).toHaveURL(new RegExp(`token_in=${USDC}&token_out=${CUSTOM}`));
  await page.reload();
  await expect(page.getByRole('textbox', { name: 'Amount of USDC to sell' })).toBeVisible();
});

test('unknown shared token produces an explicit notice instead of a silent selection', async ({
  page,
}) => {
  await page.goto('/?token_in=not-a-token');
  await expect(
    page.getByRole('status').filter({ hasText: 'token from this link is not available' }),
  ).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Amount of XLM to sell' })).toBeVisible();
});

test('external activity refreshes balances without a wallet permission prompt', async ({
  page,
  app,
}) => {
  await readySwap(page);
  await expect(page.getByText('100 XLM', { exact: true })).toBeVisible();
  const permissions = await page.evaluate(
    () => window.__mockWallet.requests.filter((v) => v === 'REQUEST_ACCESS').length,
  );
  const calls = app.balanceCalls;
  app.balance = '800000000';
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect.poll(() => app.balanceCalls).toBeGreaterThan(calls);
  await expect(page.getByText('80 XLM', { exact: true })).toBeVisible();
  expect(
    await page.evaluate(
      () => window.__mockWallet.requests.filter((v) => v === 'REQUEST_ACCESS').length,
    ),
  ).toBe(permissions);
});

test('wallet rejection never submits and leaves the form usable', async ({ page, app }) => {
  await readySwap(page);
  await page.evaluate(() => {
    window.__mockWallet.reject = true;
  });
  await page.getByRole('button', { name: 'Review & swap', exact: true }).click();
  await expect(page.getByRole('main').getByRole('alert')).toContainText('cancelled or rejected');
  expect(app.builds).toHaveLength(1);
  expect(app.submissions).toHaveLength(0);
  await expect(page.getByRole('button', { name: 'Review & swap', exact: true })).toBeEnabled();
});

test('builds mapped steps, signs once, confirms, and refreshes balances', async ({ page, app }) => {
  await readySwap(page);
  app.balance = '900000000';
  app.statusErrors = 1;
  await page.getByRole('button', { name: 'Review & swap', exact: true }).click();
  await expect(page.getByText('Swap confirmed successfully.')).toBeVisible();
  await expect(page.getByText('90 XLM', { exact: true })).toBeVisible();
  expect(app.builds[0]).toMatchObject({
    user_public_key: ADDRESS,
    token_in: XLM,
    token_out: USDC,
    amount_in: '100000000',
    sub_routes: [
      {
        amount_in: '100000000',
        steps: [
          {
            dex_type: 'aquarius',
            pool_address: 'MOCK_POOL',
            token_in: XLM,
            token_out: USDC,
            in_idx: 0,
            out_idx: 1,
          },
        ],
      },
    ],
  });
  expect(app.submissions).toHaveLength(1);
  expect(await page.evaluate(() => window.__mockWallet.signCalls)).toBe(1);
  expect(await page.evaluate(() => sessionStorage.getItem('lumagg.pendingSwap'))).toBeNull();
});

test('timeout stays pending across reload and checks the original submission channel', async ({
  page,
  app,
}) => {
  await page.clock.install();
  app.status = 'NOT_FOUND';
  await readySwap(page);
  await page.getByRole('button', { name: 'Review & swap', exact: true }).click();
  await expect.poll(() => app.statusCalls).toBeGreaterThan(0);
  await page.clock.fastForward(61_000);
  await expect(
    page.getByRole('button', { name: 'Awaiting confirmation', exact: true }),
  ).toBeDisabled();
  await expect(page.getByRole('link', { name: 'View pending transaction' })).toHaveAttribute(
    'href',
    `https://stellar.expert/explorer/public/tx/${HASH}`,
  );
  await expect(page.getByRole('main').getByRole('alert')).toHaveCount(0);
  await page.evaluate(() => localStorage.removeItem('lumagg.submitViaOfficialRpc'));
  await page.reload();
  await expect(
    page.getByRole('button', { name: 'Awaiting confirmation', exact: true }),
  ).toBeDisabled();
  app.status = 'SUCCESS';
  await page.getByRole('button', { name: 'Check transaction status', exact: true }).click();
  await expect(page.getByText('Swap confirmed successfully.')).toBeVisible();
  expect(app.submissions).toHaveLength(1);
  expect(app.rpcCalls).toBe(0);
});

test('a confirmed chain failure is terminal, not a pending timeout', async ({ page, app }) => {
  app.status = 'FAILED';
  await readySwap(page);
  await page.getByRole('button', { name: 'Review & swap', exact: true }).click();
  await expect(page.getByRole('main').getByRole('alert')).toContainText(
    'Transaction failed on-chain',
  );
  await expect(page.getByRole('link', { name: 'View transaction', exact: true })).toHaveAttribute(
    'href',
    `https://stellar.expert/explorer/public/tx/${HASH}`,
  );
  expect(await page.evaluate(() => sessionStorage.getItem('lumagg.pendingSwap'))).toBeNull();
});

for (const response of ['lost', 'missing-hash'] as const) {
  test(`a ${response} broadcast response keeps the local hash and never resubmits`, async ({
    page,
    app,
  }) => {
    app.submitResponse = response;
    await readySwap(page);
    await page.getByRole('button', { name: 'Review & swap', exact: true }).click();
    await expect(
      page.getByRole('button', { name: 'Awaiting confirmation', exact: true }),
    ).toBeDisabled();
    await expect(page.getByRole('main').getByRole('alert')).toHaveCount(0);
    expect(
      await page.evaluate(() => JSON.parse(sessionStorage.getItem('lumagg.pendingSwap')!).hash),
    ).toBe(HASH);
    await page.reload();
    await page.getByRole('button', { name: 'Check transaction status', exact: true }).click();
    await expect(page.getByText('Swap confirmed successfully.')).toBeVisible();
    expect(app.submissions).toHaveLength(1);
    expect(await page.evaluate(() => window.__mockWallet.signCalls)).toBe(0);
  });
}

test('a lost official RPC response also retains the local hash', async ({ page, app }) => {
  app.submitResponse = 'lost';
  await readySwap(page);
  await page.evaluate(() => localStorage.removeItem('lumagg.submitViaOfficialRpc'));
  await page.getByRole('button', { name: 'Review & swap', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Awaiting confirmation', exact: true }),
  ).toBeDisabled();
  await expect(page.getByRole('main').getByRole('alert')).toHaveCount(0);
  await page.reload();
  await page.getByRole('button', { name: 'Check transaction status', exact: true }).click();
  await expect(page.getByText('Swap confirmed successfully.')).toBeVisible();
  expect(app.rpcSubmissions).toBe(1);
  expect(app.submissions).toHaveLength(0);
});

test('official RPC confirmation tolerates unrelated unsupported XDR fields', async ({
  page,
  app,
}) => {
  await readySwap(page);
  await page.evaluate(() => localStorage.removeItem('lumagg.submitViaOfficialRpc'));
  await page.getByRole('button', { name: 'Review & swap', exact: true }).click();
  await expect(page.getByText('Swap confirmed successfully.')).toBeVisible();
  expect(app.rpcSubmissions).toBe(1);
  expect(app.rpcCalls).toBeGreaterThan(1);
  expect(app.statusCalls).toBe(0);
  expect(app.submissions).toHaveLength(0);
});

test('stopping recovery requires confirmation and never cancels or resubmits a transaction', async ({
  page,
  app,
}) => {
  await readySwap(page);
  await page.evaluate(
    ({ hash, account }) =>
      sessionStorage.setItem(
        'lumagg.pendingSwap',
        JSON.stringify({
          hash,
          account,
          via: 'lumagg',
          network: 'public',
        }),
      ),
    { hash: HASH, account: ADDRESS },
  );
  await page.reload();
  await expect(
    page.getByRole('button', { name: 'Awaiting confirmation', exact: true }),
  ).toBeDisabled();
  page.once('dialog', (dialog) => dialog.dismiss());
  await page.getByRole('button', { name: 'Stop tracking', exact: true }).click();
  await expect(page.getByRole('link', { name: 'View pending transaction' })).toBeVisible();
  page.once('dialog', async (dialog) => {
    expect(dialog.message()).toContain('does not cancel the transaction');
    await dialog.accept();
  });
  await page.getByRole('button', { name: 'Stop tracking', exact: true }).click();
  await expect(page.getByRole('link', { name: 'View pending transaction' })).toHaveCount(0);
  expect(await page.evaluate(() => sessionStorage.getItem('lumagg.pendingSwap'))).toBeNull();
  expect(app.submissions).toHaveLength(0);
  expect(app.rpcSubmissions).toBe(0);
});

test.describe('anonymous visitor', () => {
  test.use({ connected: false });
  test('does not contact the wallet until an explicit connect action', async ({ page, app }) => {
    await page.goto('/');
    await expect(
      page.getByRole('button', { name: 'Connect wallet to swap', exact: true }),
    ).toBeEnabled();
    expect(await page.evaluate(() => window.__mockWallet.requests)).toEqual([]);
    await page.getByRole('button', { name: 'Connect wallet to swap', exact: true }).click();
    await page.getByText('Freighter', { exact: true }).click();
    await expect(page.getByRole('button', { name: 'Wallet menu' })).toBeVisible();
    expect(await page.evaluate(() => window.__mockWallet.requests)).toContain('REQUEST_ACCESS');
    expect(app.builds).toHaveLength(0);
  });
});

test('light/dark preference survives reload and both layouts fit the viewport', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Switch to light mode', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: 'Switch to dark mode', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
});

import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
import { setTimeout as wait } from 'node:timers/promises';

const { chromium } = createRequire(import.meta.url)('playwright');
const base = 'http://127.0.0.1:3100';
const output = process.env.PROBE_OUTPUT_DIR;
const phase = process.env.PROBE_PHASE;
const rows = [];
mkdirSync(output, { recursive: true });
let server, browser, log = '';
const record = name => rows.push({ name, status: 'PASS' });
try {
  assert(!(await fetch(base + '/healthz').then(() => true, () => false)), '測試埠須空閒');
  server = spawn(process.env.ANALYTICS_BINARY, [], {
    cwd: process.env.DAY68_PRODUCT_ROOT, env: {
      ...process.env,
      ANALYTICS_DATABASE_URL: 'postgres://day65_accept@127.0.0.1:55465/' + phase + '_modes68',
      ANALYTICS_CHAT_TOKEN: 'fixture-chat', ANALYTICS_PET_TOKEN: 'fixture-pet'
    }
  });
  server.stdout.on('data', chunk => log += chunk);
  server.stderr.on('data', chunk => log += chunk);
  let ready = false;
  for (let i = 0; i < 150; i++) {
    assert.equal(server.exitCode, null, log);
    ready = await fetch(base + '/healthz').then(r => r.ok, () => false);
    if (ready) break;
    await wait(100);
  }
  assert(ready, 'Analytics 未啟動');
  browser = await chromium.launch({
    headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE ??
      '/home/willie/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome'
  });
  const page = await browser.newPage();
  async function login(password, next) {
    await page.goto(base + '/login');
    await page.locator('[name=username]').fill('admin');
    await page.locator('[name=password]').fill(password);
    await Promise.all([page.waitForURL(base + next), page.getByRole('button', { name: '登入', exact: true }).click()]);
  }
  await login('admin', '/change-password');
  for (const [name, value] of Object.entries({
    current_password: 'admin', new_password: 'Synthetic-day70-refresh',
    confirm_password: 'Synthetic-day70-refresh', email: 'day70@example.test'
  })) {
    await page.locator(`form[action="/change-password"] [name=${name}]`).fill(value);
  }
  await Promise.all([page.waitForURL(base + '/login'), page.locator('form[action="/change-password"] button[type=submit]').click()]);
  await login('Synthetic-day70-refresh', '/dashboard');
  async function ingest(type, time = '2026-11-10T00:00:00Z') {
    const response = await fetch(base + '/api/v1/events', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: 'Bearer fixture-chat' },
      body: JSON.stringify({ schema_version: 1, source_app: 'web_chat', event_id: crypto.randomUUID(), event_type: type, occurred_at: time })
    });
    assert.equal(response.status, 201);
  }
  for (let i = 0; i < 26; i++) await ingest(`notice.${i.toString().padStart(2, '0')}`);
  const query = { source: 'web_chat', event_type: 'NoTiCe', page: '2', page_size: '25', start: '2026-11-10', end: '2026-11-11' };
  await page.goto(base + '/dashboard?' + new URLSearchParams(query));
  const refresh = page.getByRole('link', { name: '重新整理報表', exact: true });
  await refresh.waitFor({ timeout: 4000 });
  const url = new URL(await refresh.getAttribute('href'), base);
  for (const [name, value] of Object.entries(query)) assert.equal(url.searchParams.get(name), value);
  assert.equal(url.hash, '');
  record('重新整理保留來源、模糊文字、第 2 頁、筆數及 UTC 日期，不跳錨點');
  await ingest('notice.00');
  await ingest('notice.00', '2026-11-11T00:00:00Z');
  const count = async () => Number(await page.locator('.daily-column strong').innerText());
  assert.equal(await count(), 26);
  const [response] = await Promise.all([page.waitForResponse(r => r.url() === url.href), refresh.click()]);
  assert.equal(response.status(), 200);
  assert.equal(await count(), 27);
  assert.equal(new URL(page.url()).searchParams.get('page'), '2');
  await page.screenshot({ path: output + '/refresh.png', fullPage: true });
  record('刷新讀回剛寫入的範圍內事件並排除迄日事件');
  await page.locator('.legacy-statistics > summary').click();
  const pager = page.getByRole('navigation', { name: '報表分頁' });
  const first = pager.getByRole('link', { name: '回到第一頁', exact: true });
  await first.waitFor({ timeout: 4000 });
  const firstURL = new URL(await first.getAttribute('href'), base);
  for (const [name, value] of Object.entries({ ...query, page: '1' })) assert.equal(firstURL.searchParams.get(name), value);
  assert.equal(firstURL.hash, '#report-results-title');
  await first.click();
  await page.waitForURL(u => u.searchParams.get('page') === '1');
  assert.equal(await pager.getByRole('link', { name: '回到第一頁', exact: true }).count(), 0);
  assert.equal(await count(), 27);
  assert(await pager.isVisible());
  assert.match(await pager.innerText(), /第 1 \/ 2 頁/);
  record('ANALYTICS-71-一鍵回第一頁，結果維持展開並保留日期及條件');
} catch (error) {
  rows.push({ name: '重新整理報表', status: 'FAIL', error: String(error) });
  console.error(error);
  process.exitCode = 1;
} finally {
  if (browser) await browser.close();
  if (server && server.exitCode === null) { const end = once(server, 'exit'); server.kill('SIGTERM'); await end; }
  writeFileSync(output + '/results.json', JSON.stringify(rows, null, 2) + '\n');
  writeFileSync(output + '/server.log', log);
  console.log(rows);
}
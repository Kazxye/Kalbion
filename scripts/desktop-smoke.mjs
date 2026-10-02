import assert from 'node:assert/strict';
import { copyFile, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import path from 'node:path';

// Stand-in for the Albion Data Project, so price refreshes are deterministic and offline.
// The app reaches it only when tauri-driver was started with KALBION_ADP_URL (debug builds).
const adpPort = Number(process.env.KALBION_ADP_PORT || 4448);
// Debug builds answer "open file" dialogs (which WebDriver cannot drive) with files from
// this directory; tauri-driver must be started with the same KALBION_TEST_DIALOG_DIR.
const dialogDir = process.env.KALBION_TEST_DIALOG_DIR;
assert.ok(
  dialogDir,
  'set KALBION_TEST_DIALOG_DIR (same value for tauri-driver and this script)',
);
const fixtures = path.resolve('crates/kalbion-capture/tests/fixtures');
await copyFile(
  path.join(fixtures, 'synthetic-items.json'),
  path.join(dialogDir, 'items.json'),
);
// A capture belongs to one session, so every run gets its own: shifting the first packet's
// time changes the capture's identity (its fingerprint covers the first packet).
const capturePcap = await readFile(path.join(fixtures, 'synthetic-loot.pcap'));
capturePcap.writeUInt32LE(
  capturePcap.readUInt32LE(24) + (Date.now() % 3600),
  24,
);
await writeFile(path.join(dialogDir, 'capture.pcap'), capturePcap);
const adpRequests = [];
const observed = new Date(Date.now() - 2 * 3_600_000)
  .toISOString()
  .slice(0, 19);
const adp = createServer((request, response) => {
  adpRequests.push(request.url);
  const url = new URL(request.url, 'http://adp');
  const items = decodeURIComponent(
    url.pathname.replace('/api/v2/stats/prices/', '').replace('.json', ''),
  ).split(',');
  const qualities = url.searchParams.get('qualities').split(',').map(Number);
  const city = url.searchParams.get('locations');
  response.setHeader('Content-Type', 'application/json');
  response.end(
    JSON.stringify(
      items.flatMap((item_id) =>
        qualities.map((quality) => ({
          item_id,
          city,
          quality,
          sell_price_min: 1000 + quality * 10,
          sell_price_min_date: observed,
        })),
      ),
    ),
  );
});
await new Promise((resolve) => adp.listen(adpPort, '127.0.0.1', resolve));

const endpoint = process.env.KALBION_WEBDRIVER_URL || 'http://127.0.0.1:4446';
let session;
async function call(method, route, body) {
  const response = await fetch(endpoint + route, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(30000),
  });
  const result = await response.json();
  if (result.value?.error) throw new Error(JSON.stringify(result.value));
  return result.value;
}
const execute = (script, args = []) =>
  call('POST', `/session/${session}/execute/sync`, { script, args });
let step = 'start';
async function until(check) {
  for (let attempt = 0; attempt < 50; attempt++) {
    if (await check()) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for UI after step: ${step}`);
}
async function click(label) {
  step = `click ${label}`;
  await until(() =>
    execute(
      `const scope = document.querySelector('[role="dialog"]') ?? document.querySelector('[role="menu"]') ?? document; const label = (button) => [...button.childNodes].filter((node) => !node.classList?.contains('count')).map((node) => node.textContent).join('').trim(); const button = [...scope.querySelectorAll('button')].find((button) => label(button) === arguments[0]); if (!button || button.disabled) return false; button.click(); return true;`,
      [label],
    ),
  );
}
async function input(selector, value) {
  step = `input ${selector}`;
  const element = await call('POST', `/session/${session}/element`, {
    using: 'css selector',
    value: selector,
  });
  const elementId = element['element-6066-11e4-a52e-4f735466cecf'];
  await call('POST', `/session/${session}/element/${elementId}/clear`, {});
  await call('POST', `/session/${session}/element/${elementId}/value`, {
    text: value,
  });
  // WebKit can return before queued keyboard events reach the input.
  await until(
    async () =>
      (await execute(`return document.querySelector(arguments[0]).value`, [
        selector,
      ])) === value,
  );
}
// Typing several KB key by key makes WebKitWebDriver drop the connection intermittently
// (app stays alive). Large payloads are pasted: native setter plus the input event React uses.
async function paste(selector, value) {
  step = `paste ${selector}`;
  await execute(
    `const element = document.querySelector(arguments[0]); Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element), 'value').set.call(element, arguments[1]); element.dispatchEvent(new Event('input', { bubbles: true }));`,
    [selector, value],
  );
  await until(
    async () =>
      (await execute(`return document.querySelector(arguments[0]).value`, [
        selector,
      ])) === value,
  );
}
async function start() {
  const result = await call('POST', '/session', {
    capabilities: {
      alwaysMatch: {
        'tauri:options': {
          application: path.resolve('target/debug/kalbion'),
        },
      },
    },
  });
  session = result.sessionId;
  await until(() =>
    execute(
      `return [...document.querySelectorAll('button')].some(button => button.textContent.trim() === 'Nova sessão' && !button.disabled)`,
    ),
  );
}
// Screenshots are visual evidence, not assertions. Some WebKitWebDriver/Wayland setups
// stall on them, so they are opt-in and a failure only warns.
async function capture(file) {
  if (process.env.KALBION_SCREENSHOTS !== '1') return;
  try {
    const image = await call('GET', `/session/${session}/screenshot`);
    await writeFile(file, Buffer.from(image, 'base64'));
  } catch (error) {
    console.warn(`WARN: screenshot ${file} skipped: ${error.message}`);
  }
}
async function ipc(request) {
  const result = await call('POST', `/session/${session}/execute/async`, {
    script: `const done=arguments[arguments.length-1]; window.__TAURI_INTERNALS__.invoke('dispatch',{request:arguments[0]}).then(value=>done({value})).catch(error=>done({error:String(error)}));`,
    args: [request],
  });
  if (result.error) throw new Error(result.error);
  return result.value;
}
try {
  await start();
  await click('Nova sessão');
  await input('input[name="name"]', 'Desktop verification');
  await click('Criar sessão');
  await until(() =>
    execute(`return document.body.textContent.includes('Gerar simulação')`),
  );
  await click('Gerar simulação');
  await until(() =>
    execute(`return document.querySelectorAll('tbody tr').length === 7`),
  );
  // Icons need the network; offline they fall back to the generic icon. Either way no
  // image may stay broken.
  await until(() =>
    execute(
      `return [...document.querySelectorAll('.item-icon img')].every(img => img.complete)`,
    ),
  );
  const icons = await execute(
    `const images = [...document.querySelectorAll('.item-icon img')]; return { loaded: images.filter(img => img.naturalWidth > 0).length, broken: images.filter(img => img.naturalWidth === 0).length, fallback: document.querySelectorAll('.item-icon:not(.with-image)').length }`,
  );
  assert.equal(icons.broken, 0, 'no broken item icons');
  console.log(
    `icons: ${icons.loaded} loaded, ${icons.fallback} generic fallback`,
  );
  const boot = await ipc({ operation: 'bootstrap' });
  assert.equal(boot.license.state, 'disabled');
  const selected = boot.sessions.find(
    (entry) => entry.name === 'Desktop verification',
  );
  assert.ok(selected);
  const sessionId = selected.id;
  await click('Definir preço');
  await input('input[name="amount"]', '100');
  await click('Salvar preço');
  await until(() =>
    execute(`return !document.querySelector('[role="dialog"]')`),
  );
  await click('Atualizar preços');
  step = 'market price refresh';
  await until(() =>
    execute(
      `return document.body.textContent.includes('Albion Data Project: ') || document.querySelector('[role="alert"]')?.textContent`,
    ),
  );
  assert.ok(
    adpRequests.length > 0,
    `the app did not call the mock market; start tauri-driver with KALBION_ADP_URL=http://127.0.0.1:${adpPort}`,
  );
  assert.ok(
    adpRequests.every((url) =>
      url.includes(`locations=${encodeURIComponent(selected.city)}`),
    ),
    'refresh uses the session market',
  );
  const priced = await ipc({
    operation: 'view',
    session_id: sessionId,
    filter: {},
  });
  for (const row of priced.rows) {
    if (row.price?.source === 'manual') {
      assert.equal(row.price.unit_silver, 100, 'manual price prevails');
    } else {
      // Simulated resources have no quality and are quoted as quality 1.
      assert.equal(row.event.quality === null, !row.event.item.has_quality);
      assert.equal(row.price.source, 'albion_data');
      assert.equal(row.price.unit_silver, 1000 + (row.event.quality ?? 1) * 10);
      assert.equal(row.price.observed_at, `${observed}.000000Z`);
    }
  }
  // Every simulated item can be quoted; the manual price kept one of them.
  const marketPriced = priced.rows.filter(
    (row) => row.price?.source === 'albion_data',
  ).length;
  assert.equal(marketPriced, 6);
  assert.equal(
    await execute(
      `return [...document.querySelectorAll('.quality.not-applicable')].length`,
    ),
    2,
    'resources show "Não se aplica"',
  );
  assert.ok(
    await execute(
      `return document.body.textContent.includes('Albion Data Project: ' + arguments[0] + ' preços atualizados')`,
      [marketPriced],
    ),
    'refresh summary shown',
  );
  await until(() =>
    execute(`return document.body.textContent.includes('ADP, há 2 h')`),
  );
  await click('Por jogador');
  await until(() =>
    execute(`return document.querySelectorAll('tbody tr').length === 3`),
  );
  await click('Acertos da sessão');
  await click('Novo lançamento');
  await input('input[name="player"]', 'Kazz');
  await input('input[name="description"]', 'Venda confirmada');
  await input('input[name="amount"]', '1001');
  await click('Registrar lançamento');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && document.body.textContent.includes('Venda confirmada')`,
    ),
  );
  await click('Dividir saldo');
  await click('Calcular divisão');
  await until(() =>
    execute(
      `return document.body.textContent.includes('Confirmar pagamentos')`,
    ),
  );
  await click('Confirmar pagamentos');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && document.querySelectorAll('tbody tr').length === 4`,
    ),
  );
  const view = await ipc({
    operation: 'view',
    session_id: sessionId,
    filter: {},
  });
  assert.equal(view.rows.length, 7);
  assert.equal(view.finance.income, 1001);
  assert.equal(view.finance.available, 0);
  assert.equal(view.finance.settlements, 1001);
  await click('Estornar');
  await click('Registrar estorno');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && document.body.textContent.includes('Estornado')`,
    ),
  );
  const reversed = await ipc({
    operation: 'view',
    session_id: sessionId,
    filter: {},
  });
  assert.equal(reversed.ledger.length, 5);
  assert.equal(reversed.finance.settlements, 668);
  assert.equal(reversed.finance.available, 333);
  const replay = await ipc({
    operation: 'import',
    session_id: sessionId,
    json: JSON.stringify({
      schema_version: 2,
      events: view.rows.map((row) => row.event),
    }),
  });
  assert.equal(replay.duplicates, 7);
  await click('Loot');
  await until(() =>
    execute(`return document.querySelectorAll('tbody tr').length === 7`),
  );
  await input('input[aria-label="Filtrar jogador"]', 'Luna');
  await until(() =>
    execute(`return document.querySelectorAll('tbody tr').length === 2`),
  );
  await input('input[aria-label="Filtrar jogador"]', 'Nobody');
  await until(() =>
    execute(
      `return document.body.textContent.includes('Nenhum loot corresponde aos filtros')`,
    ),
  );
  await click('Limpar filtros');
  await until(() =>
    execute(`return document.querySelectorAll('tbody tr').length === 7`),
  );
  await capture('/tmp/kalbion-desktop.png');
  await execute(`document.querySelector('.loot-table').scrollIntoView()`);
  await capture('/tmp/kalbion-desktop-table.png');
  await click('Ações da sessão');
  await click('Importar JSON');
  await input('textarea', '{"schema_version":99,"events":[]}');
  await click('Validar e importar');
  await until(() =>
    execute(
      `return document.querySelector('[role="dialog"] [role="alert"]')?.textContent.includes('Versão incompatível')`,
    ),
  );
  await paste(
    'textarea',
    JSON.stringify({
      schema_version: 2,
      events: view.rows.map((row) => row.event),
    }),
  );
  await click('Validar e importar');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && document.body.textContent.includes('7 duplicados ignorados')`,
    ),
  );
  await click('Registrar loot');
  await input('input[name="player"]', 'ManualTester');
  await click('Registrar loot');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && document.querySelectorAll('tbody tr').length === 8`,
    ),
  );
  await click('Anular');
  await click('Anular registro');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && document.body.textContent.includes('Anulado')`,
    ),
  );
  const afterVoid = await ipc({
    operation: 'view',
    session_id: sessionId,
    filter: {},
  });
  assert.equal(afterVoid.rows.length, 8);
  assert.equal(afterVoid.full_totals.session.events, 7);
  await click('Restaurar');
  await click('Restaurar registro');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && !document.body.textContent.includes('Anulado')`,
    ),
  );
  assert.equal(
    (await ipc({ operation: 'view', session_id: sessionId, filter: {} }))
      .full_totals.session.events,
    8,
  );
  // Offline capture import (synthetic file): needs the imported catalog first.
  await click('Configurações');
  await click('Importar items.json');
  await until(() =>
    execute(
      `return document.body.textContent.includes('Catálogo importado: 4 itens')`,
    ),
  );
  await click('Loot');
  for (const expected of [
    '3 inseridos, 0 duplicados',
    '0 inseridos, 3 duplicados',
  ]) {
    await click('Ações da sessão');
    await click('Importar captura (PCAP)');
    await until(() =>
      execute(
        `return ['Kazz', 'Luna', 'Thorin'].every(name => document.querySelector('textarea[name="roster"]')?.value.includes(name))`,
      ),
    );
    await click('Escolher arquivo e importar');
    step = `capture import: ${expected}`;
    await until(() =>
      execute(
        `return !document.querySelector('[role="dialog"]') && document.body.textContent.includes(arguments[0])`,
        [`capture.pcap: ${expected}`],
      ),
    );
  }
  const notice = await execute(
    `return document.querySelector('.notice, [role="status"]')?.textContent ?? document.body.textContent`,
  );
  assert.ok(notice.includes('1 loot de jogadores fora da lista'), notice);
  assert.ok(notice.includes('1 item fora do catálogo'), notice);
  const captured = (
    await ipc({ operation: 'view', session_id: sessionId, filter: {} })
  ).rows.filter((row) => row.event.source === 'kalbion.capture.pcap');
  assert.equal(captured.length, 3);
  assert.ok(
    captured.every(
      (row) =>
        row.event.origin === 'observed' &&
        row.imported &&
        row.event.quality === null &&
        row.event.player !== 'Inimigo',
    ),
  );
  assert.deepEqual(captured.map((row) => row.event.item.id).sort(), [
    'T4_BAG',
    'T4_WOOD',
    'T5_PLANKS_LEVEL1@1',
  ]);
  await until(() =>
    execute(
      `return document.querySelectorAll('.loot-table tbody tr').length === 11`,
    ),
  );
  // Switch to a second session and back through the header selector.
  await click('Nova sessão');
  await input('input[name="name"]', 'Second session');
  await click('Criar sessão');
  await until(() =>
    execute(
      `return document.querySelector('#session-select').selectedOptions[0]?.textContent === 'Second session' && document.body.textContent.includes('Nenhum loot por aqui')`,
    ),
  );
  await execute(
    `const select = document.querySelector('#session-select'); select.value = arguments[0]; select.dispatchEvent(new Event('change', { bubbles: true }));`,
    [sessionId],
  );
  await until(() =>
    execute(
      `return document.querySelectorAll('.loot-table tbody tr').length === 11`,
    ),
  );
  await click('Ações da sessão');
  await click('Encerrar sessão');
  await until(() =>
    execute(
      `return [...document.querySelectorAll('button')].some(button => button.textContent.trim() === 'Gerar simulação' && button.disabled)`,
    ),
  );
  await click('Ações da sessão');
  await click('Reabrir sessão');
  await until(() =>
    execute(
      `return [...document.querySelectorAll('button')].some(button => button.textContent.trim() === 'Gerar simulação' && !button.disabled)`,
    ),
  );
  await click('Configurações');
  await execute(
    `const select = document.querySelector('select[name="city"]'); select.value = 'Martlock'; select.dispatchEvent(new Event('change', { bubbles: true }));`,
  );
  await click('Salvar preferências');
  await until(
    async () =>
      (await ipc({ operation: 'bootstrap' })).settings.city === 'Martlock',
  );
  const beforeRestart = await ipc({
    operation: 'view',
    session_id: sessionId,
    filter: {},
  });
  await call('DELETE', `/session/${session}`);
  session = undefined;
  await start();
  const recovered = await ipc({
    operation: 'view',
    session_id: sessionId,
    filter: {},
  });
  assert.equal(recovered.rows.length, 11);
  assert.equal(recovered.full_totals.session.events, 11);
  assert.equal(recovered.finance.settlements, 668);
  assert.equal(
    recovered.rows.filter((row) => row.price).length,
    beforeRestart.rows.filter((row) => row.price).length,
  );
  assert.equal(
    (await ipc({ operation: 'bootstrap' })).settings.city,
    'Martlock',
  );
  console.log(
    'PASS: desktop UI, real IPC, simulation, manual loot, catalog search, item icons, void/restore, ledger reversal, session switching, import validation/replay, offline capture import (synthetic PCAP) and replay, manual and market prices, player totals, ledger, split, filters, empty/error states, session close/reopen, settings, disabled licensing, persistence after process restart.',
  );
} catch (error) {
  console.error(`FAILED at step: ${step}`);
  throw error;
} finally {
  if (session) await call('DELETE', `/session/${session}`).catch(() => {});
  adp.close();
}

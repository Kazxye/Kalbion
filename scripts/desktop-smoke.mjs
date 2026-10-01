import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';

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
async function until(check) {
  for (let attempt = 0; attempt < 50; attempt++) {
    if (await check()) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error('Timed out waiting for UI');
}
async function click(label) {
  const found = await execute(
    `const button = [...document.querySelectorAll('button')].find(button => button.textContent.trim() === arguments[0]); if (!button || button.disabled) return false; button.click(); return true;`,
    [label],
  );
  assert.ok(found, `Enabled button: ${label}`);
}
async function input(selector, value) {
  const element = await call('POST', `/session/${session}/element`, {
    using: 'css selector',
    value: selector,
  });
  const elementId = element['element-6066-11e4-a52e-4f735466cecf'];
  await call('POST', `/session/${session}/element/${elementId}/clear`, {});
  await call('POST', `/session/${session}/element/${elementId}/value`, {
    text: value,
  });
}
async function start() {
  const result = await call('POST', '/session', {
    capabilities: {
      alwaysMatch: {
        'tauri:options': {
          application: path.resolve('src-tauri/target/debug/kalbion'),
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
  const boot = await ipc({ operation: 'bootstrap' });
  assert.equal(boot.license.state, 'disabled');
  const selected = boot.sessions.find(
    (entry) => entry.name === 'Desktop verification',
  );
  assert.ok(selected);
  const sessionId = selected.id;
  await click('+ Definir preço');
  await input('input[name="amount"]', '100');
  await click('Salvar estimativa');
  await until(() =>
    execute(`return !document.querySelector('[role="dialog"]')`),
  );
  await click('Por jogador');
  await until(() =>
    execute(`return document.querySelectorAll('tbody tr').length === 3`),
  );
  await click('Acertos da sessão');
  await click('Lançamento');
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
  const replay = await ipc({
    operation: 'import',
    session_id: sessionId,
    json: JSON.stringify({
      schema_version: 1,
      events: view.rows.map((row) => row.event),
    }),
  });
  assert.equal(replay.duplicates, 7);
  await click('Loot e sessões');
  await execute(`document.querySelector('.tabs button').click()`);
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
      `return document.body.textContent.includes('Nenhum loot por aqui')`,
    ),
  );
  await execute(
    `document.querySelector('button[title="Limpar filtros"]').click()`,
  );
  await until(() =>
    execute(`return document.querySelectorAll('tbody tr').length === 7`),
  );
  const screenshot = await call('GET', `/session/${session}/screenshot`);
  await writeFile(
    '/tmp/kalbion-desktop.png',
    Buffer.from(screenshot, 'base64'),
  );
  await execute(`document.querySelector('.panel').scrollIntoView()`);
  await writeFile(
    '/tmp/kalbion-desktop-table.png',
    Buffer.from(await call('GET', `/session/${session}/screenshot`), 'base64'),
  );
  await click('Importar JSON');
  await input('textarea', '{"schema_version":99,"events":[]}');
  await click('Validar e importar');
  await until(() =>
    execute(
      `return document.querySelector('[role="dialog"] [role="alert"]')?.textContent.includes('Versão incompatível')`,
    ),
  );
  await input(
    'textarea',
    JSON.stringify({
      schema_version: 1,
      events: view.rows.map((row) => row.event),
    }),
  );
  await click('Validar e importar');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && document.body.textContent.includes('7 duplicados ignorados')`,
    ),
  );
  await click('Loot manual');
  await input('input[name="player"]', 'ManualTester');
  await click('Registrar loot');
  await until(() =>
    execute(
      `return !document.querySelector('[role="dialog"]') && document.querySelectorAll('tbody tr').length === 8`,
    ),
  );
  await click('Encerrar sessão');
  await until(() =>
    execute(
      `return [...document.querySelectorAll('button')].some(button => button.textContent.trim() === 'Gerar simulação' && button.disabled)`,
    ),
  );
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
  assert.equal(recovered.rows.length, 8);
  assert.equal(recovered.finance.settlements, 1001);
  assert.equal(
    recovered.rows.filter((row) => row.price).length,
    beforeRestart.rows.filter((row) => row.price).length,
  );
  assert.equal(
    (await ipc({ operation: 'bootstrap' })).settings.city,
    'Martlock',
  );
  console.log(
    'PASS: desktop UI, real IPC, simulation, manual loot, import validation/replay, prices, player totals, ledger, split, filters, empty/error states, session close/reopen, settings, disabled licensing, persistence after process restart. Screenshot: /tmp/kalbion-desktop.png',
  );
} finally {
  if (session) await call('DELETE', `/session/${session}`);
}

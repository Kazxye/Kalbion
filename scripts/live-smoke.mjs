// Desktop check of the live capture: real app, real IPC, real helper (kalbion-sniffer) on
// the loopback interface, fed with the synthetic Photon datagrams of the test fixture sent
// from UDP port 5056. Needs a helper allowed to capture (root, or
// `sudo setcap cap_net_raw=eip target/debug/kalbion-sniffer`) and tauri-driver started with
// the same KALBION_TEST_DIALOG_DIR. Nothing here touches the game or real traffic.
import assert from 'node:assert/strict';
import { createSocket } from 'node:dgram';
import { copyFile, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

const dialogDir = process.env.KALBION_TEST_DIALOG_DIR;
assert.ok(
  dialogDir,
  'set KALBION_TEST_DIALOG_DIR (same value for tauri-driver and this script)',
);
const networkInterface = process.env.KALBION_LIVE_INTERFACE || 'lo';
const fixtures = path.resolve('crates/kalbion-capture/tests/fixtures');
await copyFile(
  path.join(fixtures, 'synthetic-items.json'),
  path.join(dialogDir, 'items.json'),
);

/** UDP payloads of the fixture (classic PCAP, Ethernet, IPv4). */
async function payloads() {
  const file = await readFile(path.join(fixtures, 'synthetic-loot.pcap'));
  const result = [];
  let offset = 24;
  while (offset < file.length) {
    const length = file.readUInt32LE(offset + 8);
    const frame = file.subarray(offset + 16, offset + 16 + length);
    offset += 16 + length;
    const ipHeader = (frame[14] & 0x0f) * 4;
    result.push(frame.subarray(14 + ipHeader + 8));
  }
  return result;
}

const endpoint = process.env.KALBION_WEBDRIVER_URL || 'http://127.0.0.1:4446';
let session;
let step = 'start';
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
async function until(check, attempts = 100) {
  for (let attempt = 0; attempt < attempts; attempt++) {
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
/** Sets a form control the way React sees user input. */
async function set(selector, value, event = 'input') {
  step = `set ${selector}`;
  await until(() =>
    execute(
      `const element = document.querySelector(arguments[0]); if (!element) return false; Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element), 'value').set.call(element, arguments[1]); element.dispatchEvent(new Event(arguments[2], { bubbles: true })); return element.value === arguments[1];`,
      [selector, value, event],
    ),
  );
}
async function invoke(command, args = {}) {
  const result = await call('POST', `/session/${session}/execute/async`, {
    script: `const done=arguments[arguments.length-1]; window.__TAURI_INTERNALS__.invoke(arguments[0], arguments[1]).then(value=>done({value})).catch(error=>done({error:String(error)}));`,
    args: [command, args],
  });
  if (result.error) throw new Error(result.error);
  return result.value;
}

try {
  const created = await call('POST', '/session', {
    capabilities: {
      alwaysMatch: {
        'tauri:options': { application: path.resolve('target/debug/kalbion') },
      },
    },
  });
  session = created.sessionId;
  await until(() =>
    execute(
      `return [...document.querySelectorAll('button')].some(button => button.textContent.trim() === 'Nova sessão' && !button.disabled)`,
    ),
  );
  await click('Nova sessão');
  await set('input[name="name"]', 'Live verification');
  await click('Criar sessão');
  await click('Configurações');
  await click('Importar items.json');
  step = 'catalog import';
  await until(() =>
    execute(`return document.body.textContent.includes('Catálogo importado')`),
  );
  await click('Loot');

  await click('Ações da sessão');
  await click('Captura ao vivo');
  step = 'interface list';
  await until(() =>
    execute(
      `return [...document.querySelectorAll('[role="dialog"] select option')].some(option => option.value === arguments[0])`,
      [networkInterface],
    ),
  );
  await set('[role="dialog"] select', networkInterface, 'change');
  await set('textarea[name="roster"]', 'Kazz\nLuna\nThorin');
  await click('Iniciar captura');
  step = 'capture running';
  await until(() =>
    execute(
      `return document.querySelector('.live-panel.running')?.textContent.includes(arguments[0])`,
      [`Capturando em ${networkInterface}`],
    ),
  );

  step = 'send synthetic datagrams';
  const socket = createSocket('udp4');
  await new Promise((resolve) => socket.bind(5056, '127.0.0.1', resolve));
  for (const payload of await payloads()) {
    await new Promise((resolve, reject) =>
      socket.send(payload, 51000, '127.0.0.1', (error) =>
        error ? reject(error) : resolve(),
      ),
    );
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  socket.close();

  step = 'loot reaches the session';
  await until(async () => {
    const status = await invoke('live_capture_status');
    return status.inserted === 3;
  });
  const status = await invoke('live_capture_status');
  assert.equal(status.outside_roster, 1, 'Inimigo is not on the roster');
  assert.deepEqual(status.unknown_items, { 99999: 1 });
  assert.equal(status.diagnostics.loot_silver, 1);
  assert.ok(status.trace_path, 'trace enabled by default');
  await until(() =>
    execute(
      `return document.querySelectorAll('.loot-table tbody tr').length === 3`,
    ),
  );
  // The panel reads the status once per second.
  step = 'recent loot in the panel';
  await until(() =>
    execute(`return document.querySelectorAll('.live-recent li').length === 5`),
  );
  const recent = await execute(
    `return [...document.querySelectorAll('.live-recent li')].map(li => li.textContent)`,
  );
  assert.equal(recent.length, 5);
  assert.ok(
    recent.some((text) => text.includes('Índice 99999')),
    'unknown item shown by index',
  );

  if (process.env.KALBION_SCREENSHOTS === '1') {
    // Visual evidence only; some WebKitWebDriver setups stall on screenshots.
    await execute(`document.querySelector('.live-recent').open = true`);
    const image = await call('GET', `/session/${session}/screenshot`).catch(
      (error) => console.warn(`WARN: screenshot skipped: ${error.message}`),
    );
    if (image)
      await writeFile('/tmp/kalbion-live.png', Buffer.from(image, 'base64'));
  }
  await click('Parar captura');
  step = 'capture stopped';
  await until(() =>
    execute(`return !!document.querySelector('.live-panel.stopped')`),
  );
  const finalStatus = await invoke('live_capture_status');
  assert.equal(finalStatus.state, 'stopped');
  assert.equal(finalStatus.error, null);

  const boot = await invoke('dispatch', {
    request: { operation: 'bootstrap' },
  });
  const sessionId = boot.sessions.find(
    (entry) => entry.name === 'Live verification',
  ).id;
  const view = await invoke('dispatch', {
    request: { operation: 'view', session_id: sessionId, filter: {} },
  });
  const live = view.rows.filter(
    (row) => row.event.source === 'kalbion.capture.live',
  );
  assert.equal(live.length, 3);
  assert.ok(live.every((row) => row.event.origin === 'observed'));
  assert.deepEqual(live.map((row) => row.event.item.id).sort(), [
    'T4_BAG',
    'T4_WOOD',
    'T5_PLANKS_LEVEL1@1',
  ]);

  const trace = (await readFile(finalStatus.trace_path, 'utf8'))
    .trim()
    .split('\n')
    .map((line) => JSON.parse(line));
  const loot = trace.filter((record) => record.kind === 'loot');
  assert.deepEqual(
    loot.map((record) => record.outcome),
    ['inserted', 'inserted', 'inserted', 'outside_roster', 'unknown_item'],
  );
  assert.equal(trace.at(0).kind, 'start');
  assert.equal(trace.at(-1).kind, 'end');
  assert.ok(
    !JSON.stringify(trace.filter((record) => record.kind === 'event')).match(
      /Kazz|Luna|Thorin|Inimigo/,
    ),
    'event parameters carry no player names',
  );
  console.log(
    `PASS: live capture on ${networkInterface}: helper, PCAP stream, decoder, roster, catalog, session rows, status panel, stop, trace (${finalStatus.trace_path}).`,
  );
} catch (error) {
  console.error(`FAILED at step: ${step}`);
  throw error;
} finally {
  if (session) await call('DELETE', `/session/${session}`).catch(() => {});
}

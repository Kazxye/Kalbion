import { useEffect, useState } from 'react';
import { CircleAlert, Radio, Square } from 'lucide-react';
import { liveCaptureInterfaces } from './api';
import { Field } from './components';
import { ItemIcon, TierBadge } from './item';
import type { LiveInterface, LiveOutcome, LiveStatus } from './types';

const clock = (value: string) =>
  new Date(value).toLocaleTimeString('pt-BR', { hour12: false });

const outcomes: Record<LiveOutcome, string> = {
  inserted: 'Gravado',
  duplicate: 'Duplicado',
  outside_roster: 'Fora da lista',
  unknown_item: 'Item fora do catálogo',
  invalid_player: 'Jogador inválido',
  error: 'Erro ao gravar',
};

/** Prefers a running, non-loopback interface with an address, then Linux's "any". */
function preferred(interfaces: LiveInterface[]) {
  return (
    interfaces.find(
      (item) => item.running && !item.loopback && item.addresses.length > 0,
    )?.name ??
    interfaces.find((item) => item.name === 'any')?.name ??
    interfaces[0]?.name ??
    ''
  );
}

export function LiveCaptureForm({
  players,
  busy,
  submit,
}: {
  players: string[];
  busy: boolean;
  submit: (roster: string[], networkInterface: string, trace: boolean) => void;
}) {
  const [roster, setRoster] = useState(players.join('\n'));
  const [interfaces, setInterfaces] = useState<LiveInterface[]>([]);
  const [selected, setSelected] = useState('');
  const [trace, setTrace] = useState(true);
  const [listError, setListError] = useState('');
  const [loading, setLoading] = useState(true);
  const names = roster
    .split(/[\n,;]/)
    .map((name) => name.trim())
    .filter(Boolean);
  const load = () => {
    setLoading(true);
    setListError('');
    liveCaptureInterfaces()
      .then((list) => {
        setInterfaces(list);
        setSelected((current) =>
          list.some((item) => item.name === current)
            ? current
            : preferred(list),
        );
      })
      .catch((reason) => setListError(String(reason)))
      .finally(() => setLoading(false));
  };
  useEffect(load, []);
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        submit(names, selected, trace);
      }}
    >
      <p className="help">
        Lê ao vivo os pacotes que o servidor do Albion envia ao seu computador
        (UDP, porta 5056), sem modo promíscuo, sem ler memória e sem alterar o
        jogo. Só o ajudante de captura recebe permissão de rede; a decodificação
        roda no aplicativo, sem privilégios.
      </p>
      {listError && (
        <p role="alert" className="live-error">
          <CircleAlert size={16} aria-hidden />
          <span>{listError}</span>
        </p>
      )}
      <Field label="Interface de rede">
        <div className="live-interface">
          <select
            value={selected}
            disabled={loading || !interfaces.length}
            onChange={(event) => setSelected(event.target.value)}
            required
          >
            {!interfaces.length && (
              <option value="">
                {loading ? 'Carregando…' : 'Nenhuma interface'}
              </option>
            )}
            {interfaces.map((item) => (
              <option key={item.name} value={item.name}>
                {item.description
                  ? `${item.name} — ${item.description}`
                  : item.name}
                {item.addresses.length ? ` (${item.addresses[0]})` : ''}
                {item.running ? '' : ' (inativa)'}
              </option>
            ))}
          </select>
          <button type="button" disabled={loading} onClick={load}>
            Atualizar
          </button>
        </div>
      </Field>
      <Field label="Jogadores da party ou guilda (um por linha)">
        <textarea
          name="roster"
          rows={5}
          value={roster}
          onChange={(event) => setRoster(event.target.value)}
          required
        />
      </Field>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={trace}
          onChange={(event) => setTrace(event.target.checked)}
        />
        Gravar log de diagnóstico (cada loot visto e os eventos de item, com
        nomes de outros jogadores ocultados)
      </label>
      <p className="help">
        Só o loot desses jogadores é gravado na sessão; o de outros é apenas
        contado. A qualidade fica desconhecida, porque o evento de loot não a
        informa.
      </p>
      <p className="help warning-note">
        Experimental: ainda não validado com tráfego real do jogo. Não há
        autorização da Sandbox Interactive para este uso.
      </p>
      <div className="form-actions">
        <button
          className="primary"
          disabled={busy || loading || !selected || !names.length}
        >
          <Radio size={16} aria-hidden />
          Iniciar captura
        </button>
      </div>
    </form>
  );
}

function hints(status: LiveStatus) {
  const result: string[] = [];
  const diagnostics = status.diagnostics;
  const seconds = status.started_at
    ? (Date.now() - Date.parse(status.started_at)) / 1000
    : 0;
  if (status.state !== 'running' || seconds < 10 || !diagnostics) return result;
  if (diagnostics.udp_from_game_server === 0)
    result.push(
      'Nenhum pacote do servidor do Albion até agora. Confira se o jogo está aberto e se esta é a interface usada pela conexão.',
    );
  else if (diagnostics.events === 0)
    result.push(
      diagnostics.encrypted_packets + diagnostics.encrypted_messages > 0
        ? 'Chegam pacotes do Albion, mas só criptografados.'
        : 'Chegam pacotes do Albion, mas nenhum evento legível. O formato pode ter mudado.',
    );
  if (diagnostics.loot_malformed > 0)
    result.push(
      `${diagnostics.loot_malformed} eventos de loot com formato inesperado (veja o log de diagnóstico).`,
    );
  if (status.dropped_by_capture > 0)
    result.push(
      `${status.dropped_by_capture} pacotes descartados pelo sistema antes da leitura; algum loot pode ter se perdido.`,
    );
  return result;
}

/** Status of the current (or last) live capture of this session. */
export function LivePanel({
  status,
  busy,
  stop,
}: {
  status: LiveStatus;
  busy: boolean;
  stop: () => void;
}) {
  const unknown = Object.values(status.unknown_items).reduce(
    (sum, value) => sum + value,
    0,
  );
  const running = status.state === 'running';
  const diagnostics = status.diagnostics;
  return (
    <section
      className={`live-panel ${status.state}`}
      aria-label="Captura ao vivo"
    >
      <div className="live-head">
        <span className="live-state" role="status">
          <Radio size={16} aria-hidden />
          {running
            ? `Capturando em ${status.interface}`
            : status.state === 'failed'
              ? 'Captura interrompida'
              : 'Captura parada'}
        </span>
        <dl className="live-counts">
          <div>
            <dt>Gravados</dt>
            <dd>{status.inserted}</dd>
          </div>
          <div>
            <dt>Duplicados</dt>
            <dd>{status.duplicates}</dd>
          </div>
          <div>
            <dt>Fora da lista</dt>
            <dd>{status.outside_roster}</dd>
          </div>
          <div>
            <dt>Fora do catálogo</dt>
            <dd>{unknown}</dd>
          </div>
          <div>
            <dt>Pacotes do servidor</dt>
            <dd>{diagnostics?.udp_from_game_server ?? 0}</dd>
          </div>
          <div>
            <dt>Eventos</dt>
            <dd>{diagnostics?.events ?? 0}</dd>
          </div>
        </dl>
        {running && (
          <button className="small" disabled={busy} onClick={stop}>
            <Square size={14} aria-hidden />
            Parar captura
          </button>
        )}
      </div>
      {status.error && (
        <p role="alert" className="live-error">
          <CircleAlert size={16} aria-hidden />
          <span className="selectable">{status.error}</span>
        </p>
      )}
      {hints(status).map((hint) => (
        <p key={hint} className="help warning-note">
          {hint}
        </p>
      ))}
      {status.recent.length > 0 && (
        <details className="live-recent" open={running}>
          <summary>Últimos loots vistos ({status.recent.length})</summary>
          <ol>
            {status.recent.map((loot, index) => (
              <li key={`${loot.at}-${index}`} data-outcome={loot.outcome}>
                <time dateTime={loot.at}>{clock(loot.at)}</time>
                {loot.item ? (
                  <span className="live-item">
                    <ItemIcon item={loot.item} quality={null} />
                    <span>{loot.item.name}</span>
                    <TierBadge item={loot.item} />
                  </span>
                ) : (
                  <span className="live-item unknown">
                    Índice {loot.item_index}
                  </span>
                )}
                <span>× {loot.quantity}</span>
                <span>{loot.player ?? '—'}</span>
                <span className="live-outcome">{outcomes[loot.outcome]}</span>
              </li>
            ))}
          </ol>
        </details>
      )}
      {status.trace_path && (
        <p className="help">
          Log de diagnóstico:{' '}
          <span className="selectable">{status.trace_path}</span>
          {status.trace_full ? ' (limite de 50 MB atingido)' : ''}
        </p>
      )}
    </section>
  );
}

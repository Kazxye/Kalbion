import { useEffect, useState } from 'react';
import { Boxes } from 'lucide-react';
import { request } from './api';
import { Field, ItemIcon } from './components';
import { date, qualities, qualityLabel, silver, tierLabel } from './format';
import type { Item, LootRow } from './types';

export function LootTable({
  rows,
  price,
  toggleVoid,
  busy,
  closed,
}: {
  rows: LootRow[];
  price: (row: LootRow) => void;
  toggleVoid: (row: LootRow) => void;
  busy: boolean;
  closed: boolean;
}) {
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>ITEM</th>
            <th>JOGADOR</th>
            <th>TIER / ENC.</th>
            <th>QUALIDADE</th>
            <th>QTD.</th>
            <th>PREÇO UNIT.</th>
            <th>HORÁRIO / ORIGEM</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const { event } = row;
            return (
              <tr
                key={`${event.source}:${event.id}`}
                className={row.voided_at ? 'voided' : ''}
              >
                <td>
                  <div className="item-cell">
                    <ItemIcon item={event.item} quality={event.quality} />
                    <div>
                      <strong>{event.item.name}</strong>
                      <small>{event.item.id}</small>
                    </div>
                  </div>
                </td>
                <td>
                  <span className="avatar">{event.player.slice(0, 1)}</span>
                  {event.player}
                </td>
                <td>
                  <span className="tier">
                    {event.item.tier ? `T${event.item.tier}` : 'Sem tier'}
                  </span>
                  <span
                    className={`enchantment enchantment-${event.item.enchantment}`}
                  >
                    .{event.item.enchantment}
                  </span>
                </td>
                <td className={event.quality === null ? 'unknown' : ''}>
                  {qualityLabel(event.quality)}
                </td>
                <td className="numeric">{silver(event.quantity)}</td>
                <td>
                  <button
                    className={`price-button ${row.price ? 'silver' : ''}`}
                    disabled={busy}
                    onClick={() => price(row)}
                  >
                    {row.price
                      ? `${silver(row.price.unit_silver)} s`
                      : '+ Definir preço'}
                  </button>
                  {row.price && (
                    <small
                      className="price-context"
                      title={`${row.price.server} · ${row.price.city} · ${date(row.price.recorded_at)}`}
                    >
                      Manual · {row.price.city}
                      <br />
                      {date(row.price.recorded_at)}
                    </small>
                  )}
                </td>
                <td>
                  <small>{date(event.occurred_at)}</small>
                  <span
                    className={`origin ${event.origin === 'simulated' ? 'simulated' : ''}`}
                  >
                    {event.origin === 'simulated'
                      ? 'Simulado'
                      : event.origin === 'manual'
                        ? 'Manual'
                        : 'Observado (declarado)'}
                    {row.imported ? ' · Importado' : ''}
                  </span>
                  {row.voided_at && (
                    <span className="origin voided-tag">
                      Anulado · {date(row.voided_at)}
                    </span>
                  )}
                </td>
                <td>
                  <button
                    className="row-action"
                    disabled={busy || closed}
                    title={
                      closed ? 'Reabra a sessão para alterar o loot' : undefined
                    }
                    onClick={() => toggleVoid(row)}
                  >
                    {row.voided_at ? 'Restaurar' : 'Anular'}
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {!rows.length && (
        <div className="empty">
          <Boxes size={32} />
          <h3>Nenhum loot por aqui, ainda.</h3>
          <p>Gere uma simulação, registre loot manual ou ajuste os filtros.</p>
        </div>
      )}
    </div>
  );
}

export function ManualForm({
  busy,
  submit,
}: {
  busy: boolean;
  submit: (data: {
    item_id: string;
    player: string;
    quality: number | null;
    quantity: number;
  }) => void;
}) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<Item[] | null>(null);
  const [selected, setSelected] = useState('');
  const [searchError, setSearchError] = useState('');
  useEffect(() => {
    let current = true;
    const timer = setTimeout(() => {
      request<Item[]>('catalog_search', { query, limit: 50 })
        .then((items) => {
          if (!current) return;
          setResults(items);
          setSearchError('');
          setSelected((value) =>
            items.some((item) => item.id === value)
              ? value
              : (items[0]?.id ?? ''),
          );
        })
        .catch((reason) => {
          if (current) setSearchError(String(reason));
        });
    }, 150);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [query]);
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        const data = new FormData(event.currentTarget);
        const quality = String(data.get('quality'));
        submit({
          item_id: selected,
          player: String(data.get('player')),
          quality: quality === '' ? null : Number(quality),
          quantity: Number(data.get('quantity')),
        });
      }}
    >
      <Field label="Jogador">
        <input autoFocus name="player" required maxLength={64} />
      </Field>
      <Field label="Buscar no catálogo">
        <input
          name="catalog-query"
          placeholder="Nome ou ID, ex.: bolsa, T5_BAG@1"
          maxLength={150}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </Field>
      <div className="field">
        <span id="catalog-results-label">Item</span>
        <div
          className="catalog-results"
          role="listbox"
          aria-labelledby="catalog-results-label"
        >
          {results?.map((item) => (
            <button
              type="button"
              role="option"
              key={item.id}
              aria-selected={item.id === selected}
              className={item.id === selected ? 'selected' : ''}
              onClick={() => setSelected(item.id)}
            >
              <ItemIcon item={item} quality={null} />
              <span>
                <strong>{item.name}</strong>
                <small>
                  {tierLabel(item.tier, item.enchantment)} · {item.id}
                </small>
              </span>
            </button>
          ))}
        </div>
      </div>
      {results === null && !searchError && (
        <p className="help">Carregando catálogo…</p>
      )}
      {results?.length === 0 && (
        <p className="help">Nenhum item corresponde à busca.</p>
      )}
      {searchError && (
        <p role="alert" className="error">
          {searchError}
        </p>
      )}
      <div className="form-grid">
        <Field label="Qualidade">
          <select name="quality" defaultValue="1">
            {qualities.map((quality, index) => (
              <option key={quality} value={index + 1}>
                {quality}
              </option>
            ))}
            <option value="">Desconhecida</option>
          </select>
        </Field>
        <Field label="Quantidade">
          <input
            name="quantity"
            type="number"
            defaultValue="1"
            min="1"
            max="1000000"
            step="1"
            required
          />
        </Field>
      </div>
      <p className="help">
        Origem manual. Horário registrado no momento do envio. Mostra até 50
        resultados; refine a busca para encontrar outros itens.
      </p>
      <button className="primary" disabled={busy || !selected}>
        Registrar loot
      </button>
    </form>
  );
}

export function ImportForm({
  sessionId,
  busy,
  submit,
}: {
  sessionId: string;
  busy: boolean;
  submit: (json: string) => void;
}) {
  const [json, setJson] = useState('');
  const [error, setError] = useState('');
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        submit(json);
      }}
    >
      <p className="help">
        Contratos v1 e v2: schema_version e events. Máximo 5 MB / 10.000
        eventos. A origem é declarada pelo arquivo e não é certificada. O lote
        inteiro é validado antes de salvar.
      </p>
      <Field label="ID da sessão de destino">
        <input
          readOnly
          value={sessionId}
          onFocus={(event) => event.target.select()}
        />
      </Field>
      <Field label="Arquivo JSON">
        <input
          type="file"
          accept=".json,application/json"
          onChange={async (event) => {
            const file = event.target.files?.[0];
            if (!file) return;
            try {
              if (file.size > 5_000_000)
                throw new Error('Arquivo excede 5 MB.');
              const parsed = JSON.parse(await file.text()) as {
                schema_version: number;
                events: unknown[];
              };
              setJson(
                JSON.stringify(
                  {
                    schema_version: parsed.schema_version,
                    events: parsed.events,
                  },
                  null,
                  2,
                ),
              );
              setError('');
            } catch (reason) {
              setError(String(reason));
              setJson('');
            }
          }}
        />
      </Field>
      <Field label="Eventos normalizados">
        <textarea
          rows={9}
          value={json}
          onChange={(event) => setJson(event.target.value)}
          required
          placeholder={'{"schema_version":2,"events":[…]}'}
        />
      </Field>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <p className="help">
        session_id deve corresponder ao destino. Exports da mesma sessão podem
        ser reimportados; preços, anulações e acertos não são restaurados por
        esta importação.
      </p>
      <button className="primary" disabled={busy || !json}>
        Validar e importar
      </button>
    </form>
  );
}

import { useEffect, useState } from 'react';
import {
  Ban,
  Boxes,
  FlaskConical,
  Plus,
  RotateCcw,
  Search,
  X,
} from 'lucide-react';
import { request } from './api';
import { EmptyState, Field, InlineError } from './components';
import { date, qualities, silver, tierLabel } from './format';
import { ItemIcon, OriginTag, QualityMark, TierBadge } from './item';
import type { Filter, Item, LootRow, View } from './types';

export const emptyFilter: Filter = {
  player: '',
  item: '',
  tier: null,
  enchantment: null,
  quality: null,
};
const isFiltered = (filter: Filter) =>
  JSON.stringify(filter) !== JSON.stringify(emptyFilter);
const filterSelects = [
  {
    key: 'tier',
    label: 'Tier',
    all: 'Tier',
    options: [1, 2, 3, 4, 5, 6, 7, 8].map((value) => [value, `T${value}`]),
  },
  {
    key: 'enchantment',
    label: 'Encantamento',
    all: 'Encantamento',
    options: [0, 1, 2, 3, 4].map((value) => [
      value,
      value ? `.${value}` : '.0 (nenhum)',
    ]),
  },
  {
    key: 'quality',
    label: 'Qualidade',
    all: 'Qualidade',
    options: [
      ...qualities.map((label, index) => [index + 1, label]),
      [0, 'Desconhecida'],
    ],
  },
] as const;
const closedHint = 'Reabra a sessão para alterar o loot';

export function LootView({
  view,
  filter,
  setFilter,
  loading,
  busy,
  closed,
  simulate,
  register,
  price,
  toggleVoid,
}: {
  view: View;
  filter: Filter;
  setFilter: (filter: Filter) => void;
  loading: boolean;
  busy: boolean;
  closed: boolean;
  simulate: () => void;
  register: () => void;
  price: (row: LootRow) => void;
  toggleVoid: (row: LootRow) => void;
}) {
  const filtered = isFiltered(filter);
  const voided = view.rows.filter((row) => row.voided_at).length;
  return (
    <>
      <div className="toolbar">
        <div className="filters" role="search" aria-label="Filtrar loot">
          <label className="search">
            <Search size={15} aria-hidden />
            <input
              aria-label="Buscar item por nome ou ID"
              placeholder="Buscar item ou ID"
              maxLength={150}
              value={filter.item}
              onChange={(event) =>
                setFilter({ ...filter, item: event.target.value })
              }
            />
          </label>
          <input
            className="player"
            aria-label="Filtrar jogador"
            placeholder="Jogador"
            maxLength={64}
            value={filter.player}
            onChange={(event) =>
              setFilter({ ...filter, player: event.target.value })
            }
          />
          {filterSelects.map(({ key, label, all, options }) => (
            <select
              aria-label={label}
              key={key}
              value={filter[key] ?? ''}
              onChange={(event) =>
                setFilter({
                  ...filter,
                  [key]:
                    event.target.value === ''
                      ? null
                      : Number(event.target.value),
                })
              }
            >
              <option value="">{all}</option>
              {options.map(([value, text]) => (
                <option key={value} value={value}>
                  {text}
                </option>
              ))}
            </select>
          ))}
          <button
            className="ghost"
            disabled={!filtered}
            onClick={() => setFilter(emptyFilter)}
          >
            <X size={15} aria-hidden />
            Limpar filtros
          </button>
        </div>
        <div className="toolbar-actions">
          <button
            disabled={busy || closed}
            title={closed ? closedHint : undefined}
            onClick={simulate}
          >
            <FlaskConical size={15} aria-hidden />
            Gerar simulação
          </button>
          <button
            className="primary"
            disabled={busy || closed}
            title={closed ? closedHint : undefined}
            onClick={register}
          >
            <Plus size={16} aria-hidden />
            Registrar loot
          </button>
        </div>
      </div>
      <div className="status-line" aria-live="polite">
        <span>
          {view.rows.length}{' '}
          {filtered ? 'registros correspondem aos filtros' : 'registros'}
          {voided > 0 && `, ${voided} anulados`}
          {closed && '. Sessão encerrada: loot somente leitura.'}
        </span>
        <span>{loading ? 'Atualizando…' : ''}</span>
      </div>
      <LootTable
        view={view}
        price={price}
        toggleVoid={toggleVoid}
        busy={busy}
        closed={closed}
        filtered={filtered}
        loading={loading}
      />
    </>
  );
}

function LootTable({
  view,
  price,
  toggleVoid,
  busy,
  closed,
  filtered,
  loading,
}: {
  view: View;
  price: (row: LootRow) => void;
  toggleVoid: (row: LootRow) => void;
  busy: boolean;
  closed: boolean;
  filtered: boolean;
  loading: boolean;
}) {
  const totals = view.totals.session;
  return (
    <div className="table-scroll loot-table">
      <table>
        <caption className="visually-hidden">
          Registro de loot da sessão {view.session.name}
        </caption>
        <thead>
          <tr>
            <th scope="col">Item</th>
            <th scope="col">Tier</th>
            <th scope="col">Qualidade</th>
            <th scope="col" className="numeric">
              Qtd.
            </th>
            <th scope="col" className="numeric">
              Preço unit.
            </th>
            <th scope="col" className="col-player">
              Jogador
            </th>
            <th scope="col">Origem</th>
            <th scope="col">
              <span className="visually-hidden">Ações</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {view.rows.map((row) => (
            <LootRowView
              key={`${row.event.source}:${row.event.id}`}
              row={row}
              price={price}
              toggleVoid={toggleVoid}
              busy={busy}
              closed={closed}
            />
          ))}
          {loading &&
            !view.rows.length &&
            [0, 1, 2].map((index) => (
              <tr className="skeleton" key={index} aria-hidden>
                {Array.from({ length: 8 }).map((_, cell) => (
                  <td key={cell}>
                    <div />
                  </td>
                ))}
              </tr>
            ))}
        </tbody>
        {view.rows.length > 0 && (
          <tfoot>
            <tr>
              <td colSpan={3}>
                {filtered ? 'Total filtrado' : 'Total da sessão'}{' '}
                <small>(anulados não contam)</small>
              </td>
              <td className="numeric">{silver(totals.quantity)}</td>
              <td className="numeric amount">
                {silver(totals.estimated_silver)} s
              </td>
              <td colSpan={3}>
                {totals.unpriced_events > 0 &&
                  `${totals.unpriced_events} sem preço`}
              </td>
            </tr>
          </tfoot>
        )}
      </table>
      {!loading && !view.rows.length && (
        <EmptyState
          icon={<Boxes size={28} aria-hidden />}
          title={
            filtered
              ? 'Nenhum loot corresponde aos filtros'
              : 'Nenhum loot por aqui, ainda'
          }
        >
          {filtered
            ? 'Ajuste ou limpe os filtros para ver os registros.'
            : 'Registre um loot ou gere uma simulação para testar.'}
        </EmptyState>
      )}
    </div>
  );
}

function LootRowView({
  row,
  price,
  toggleVoid,
  busy,
  closed,
}: {
  row: LootRow;
  price: (row: LootRow) => void;
  toggleVoid: (row: LootRow) => void;
  busy: boolean;
  closed: boolean;
}) {
  const { event } = row;
  const action = row.voided_at ? 'Restaurar' : 'Anular';
  return (
    <tr className={row.voided_at ? 'voided' : ''}>
      <td>
        <div className="item-cell">
          <ItemIcon item={event.item} quality={event.quality} />
          <div>
            <strong>{event.item.name}</strong>
            <small>{event.item.id}</small>
            <small className="item-player">{event.player}</small>
          </div>
        </div>
      </td>
      <td>
        <TierBadge item={event.item} />
      </td>
      <td>
        <QualityMark quality={event.quality} />
      </td>
      <td className="numeric amount">{silver(event.quantity)}</td>
      <td className="numeric">
        {row.price ? (
          <span className="price-set">
            <button
              disabled={busy}
              aria-label={`Alterar preço de ${event.item.name}: ${silver(row.price.unit_silver)} silver`}
              onClick={() => price(row)}
            >
              {silver(row.price.unit_silver)} s
            </button>
            <small
              title={`Preço manual em ${row.price.city}, ${date(row.price.recorded_at)}`}
            >
              manual, {row.price.city}
            </small>
          </span>
        ) : (
          <button
            className="price-button"
            disabled={busy}
            onClick={() => price(row)}
          >
            Definir preço
          </button>
        )}
      </td>
      <td className="col-player">{event.player}</td>
      <td className="when">
        <OriginTag origin={event.origin} imported={row.imported} />
        <small>{date(event.occurred_at)}</small>
        {row.voided_at && (
          <span className="tag voided">
            <Ban size={12} aria-hidden />
            Anulado
          </span>
        )}
      </td>
      <td>
        <button
          className="ghost small"
          disabled={busy || closed}
          title={closed ? closedHint : undefined}
          aria-label={`${action} ${event.item.name} de ${event.player}`}
          onClick={() => toggleVoid(row)}
        >
          {row.voided_at ? (
            <RotateCcw size={14} aria-hidden />
          ) : (
            <Ban size={14} aria-hidden />
          )}
          {action}
        </button>
      </td>
    </tr>
  );
}

export function PlayersView({ view }: { view: View }) {
  const players = Object.entries(view.full_totals.players).sort(
    ([, a], [, b]) => b.estimated_silver - a.estimated_silver,
  );
  return (
    <>
      <p className="page-intro">
        Totais de loot por jogador nesta sessão, sem anulados. Os valores são
        estimativas pelos preços manuais, não silver recebido.
      </p>
      <div className="table-scroll">
        <table>
          <caption className="visually-hidden">
            Totais por jogador da sessão {view.session.name}
          </caption>
          <thead>
            <tr>
              <th scope="col">Jogador</th>
              <th scope="col" className="numeric">
                Registros
              </th>
              <th scope="col" className="numeric">
                Itens
              </th>
              <th scope="col" className="numeric">
                Sem preço
              </th>
              <th scope="col" className="numeric">
                Valor estimado
              </th>
            </tr>
          </thead>
          <tbody>
            {players.map(([player, total]) => (
              <tr key={player}>
                <th scope="row">{player}</th>
                <td className="numeric">{total.events}</td>
                <td className="numeric">{silver(total.quantity)}</td>
                <td className="numeric">{total.unpriced_events}</td>
                <td className="numeric amount">
                  {silver(total.estimated_silver)} s
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!players.length && (
          <EmptyState
            icon={<Boxes size={28} aria-hidden />}
            title="Nenhum jogador com loot"
          >
            Os totais aparecem aqui assim que houver loot válido na sessão.
          </EmptyState>
        )}
      </div>
    </>
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
          type="search"
          placeholder="Nome ou ID, por exemplo bolsa ou T5_BAG@1"
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
              onClick={() => setSelected(item.id)}
            >
              <ItemIcon item={item} quality={null} />
              <span>
                <strong>{item.name}</strong>
                <small>
                  {tierLabel(item.tier, item.enchantment)}, {item.id}
                </small>
              </span>
            </button>
          ))}
          {results === null && !searchError && (
            <p className="help">Carregando catálogo…</p>
          )}
          {results?.length === 0 && (
            <p className="help">Nenhum item corresponde à busca.</p>
          )}
        </div>
        <span className="help">
          Mostra até 50 resultados; refine a busca para encontrar outros itens.
        </span>
      </div>
      <InlineError message={searchError} />
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
        Origem manual. O horário é registrado no momento em que você salvar.
      </p>
      <div className="form-actions">
        <button className="primary" disabled={busy || !selected}>
          Registrar loot
        </button>
      </div>
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
        Formatos v1 e v2 (schema_version e events), até 5 MB e 10.000 eventos. A
        origem é declarada pelo arquivo e não é certificada. O lote inteiro é
        validado antes de salvar; replays idênticos são ignorados.
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
          rows={8}
          value={json}
          onChange={(event) => setJson(event.target.value)}
          required
          placeholder={'{"schema_version":2,"events":[…]}'}
        />
      </Field>
      <InlineError message={error} />
      <p className="help">
        O session_id dos eventos precisa ser o desta sessão. Preços, anulações e
        acertos não são restaurados por esta importação.
      </p>
      <div className="form-actions">
        <button className="primary" disabled={busy || !json}>
          Validar e importar
        </button>
      </div>
    </form>
  );
}

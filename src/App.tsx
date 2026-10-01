import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Archive,
  ArrowDownToLine,
  Boxes,
  Check,
  ChevronRight,
  CircleDollarSign,
  FlaskConical,
  Hammer,
  LayoutDashboard,
  Plus,
  Search,
  Settings2,
  Shield,
  Swords,
  Users,
  X,
} from 'lucide-react';
import { desktop, exportSession, importCatalog, request } from './api';
import {
  ConfirmDialog,
  Dialog,
  Field,
  Stat,
  type Confirmation,
} from './components';
import { LedgerForm, LedgerPanel, SplitForm } from './finance';
import { ImportForm, LootTable, ManualForm } from './loot';
import { date, qualities, qualityLabel, serverNames, silver } from './format';
import { SettingsPage } from './settings';
import type {
  Bootstrap,
  Filter,
  InsertResult,
  LootRow,
  Session,
  Share,
  View,
} from './types';

const emptyFilter: Filter = {
  player: '',
  item: '',
  tier: null,
  enchantment: null,
  quality: null,
};
type Page = 'loot' | 'crafting' | 'finance' | 'compositions' | 'settings';
type Modal = 'session' | 'manual' | 'import' | 'ledger' | 'split' | null;
const nav = [
  { id: 'loot', label: 'Loot e sessões', icon: Boxes },
  { id: 'crafting', label: 'Crafting', icon: Hammer },
  { id: 'finance', label: 'Financeiro', icon: CircleDollarSign },
  { id: 'compositions', label: 'Composições', icon: Users },
] as const;
const filterSelects = [
  {
    key: 'tier',
    label: 'Todos os tiers',
    options: [1, 2, 3, 4, 5, 6, 7, 8].map((value) => [value, `T${value}`]),
  },
  {
    key: 'enchantment',
    label: 'Encantamento',
    options: [0, 1, 2, 3, 4].map((value) => [value, `.${value}`]),
  },
  {
    key: 'quality',
    label: 'Qualidade',
    options: [
      ...qualities.map((label, index) => [index + 1, label]),
      [0, 'Desconhecida'],
    ],
  },
] as const;

export default function App() {
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [active, setActive] = useState('');
  const [view, setView] = useState<View | null>(null);
  const [page, setPage] = useState<Page>('loot');
  const [filter, setFilter] = useState<Filter>(emptyFilter);
  const [tab, setTab] = useState<'events' | 'players' | 'ledger'>('events');
  const [modal, setModal] = useState<Modal>(null);
  const [pricing, setPricing] = useState<LootRow | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [revision, setRevision] = useState(0);
  const sequence = useRef(0);
  const mutation = useRef(false);
  const refresh = useCallback(async () => {
    const data = await request<Bootstrap>('bootstrap');
    setBoot(data);
    setActive((current) => current || data.sessions[0]?.id || '');
    setRevision((value) => value + 1);
  }, []);
  useEffect(() => {
    refresh()
      .catch((reason) => setError(String(reason)))
      .finally(() => setLoading(false));
  }, [refresh]);
  useEffect(() => {
    if (!active) return;
    const current = ++sequence.current;
    setLoading(true);
    const timer = setTimeout(() => {
      request<View>('view', { session_id: active, filter })
        .then((data) => {
          if (sequence.current === current) setView(data);
        })
        .catch((reason) => {
          if (sequence.current === current) {
            setError(String(reason));
            setView(null);
          }
        })
        .finally(() => {
          if (sequence.current === current) setLoading(false);
        });
    }, 120);
    return () => {
      clearTimeout(timer);
      sequence.current++;
    };
  }, [active, filter, revision]);
  const closeDialogs = useCallback(() => {
    setModal(null);
    setPricing(null);
    setConfirmation(null);
  }, []);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busy) closeDialogs();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [busy, closeDialogs]);
  async function act(
    action: () => Promise<unknown>,
    message = 'Alterações salvas.',
  ) {
    if (mutation.current) return;
    mutation.current = true;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const result = await action();
      closeDialogs();
      setNotice(typeof result === 'string' ? result : message);
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      mutation.current = false;
      setBusy(false);
    }
  }
  const session = boot?.sessions.find((item) => item.id === active);
  const currentView = view?.session.id === active ? view : null;
  const args = { session_id: active };
  const closed = !!session?.closed_at;
  async function exportData(format: string) {
    await act(async () => {
      const saved = await exportSession(active, format);
      return saved
        ? `Sessão completa exportada em ${format.toUpperCase()}.`
        : 'Exportação cancelada.';
    });
  }
  function toggleVoid(row: LootRow) {
    const voided = !row.voided_at;
    const target = {
      ...args,
      source: row.event.source,
      event_id: row.event.id,
      voided,
    };
    setConfirmation(
      voided
        ? {
            title: 'Anular loot',
            message: `${row.event.player} · ${row.event.item.name} × ${silver(row.event.quantity)}. O registro continua no histórico e na exportação, mas sai dos totais. Pode ser restaurado depois.`,
            confirmLabel: 'Anular registro',
            action: () => request('set_voided', target),
            notice: 'Loot anulado; totais atualizados.',
          }
        : {
            title: 'Restaurar loot',
            message: `${row.event.player} · ${row.event.item.name} × ${silver(row.event.quantity)} volta a contar nos totais.`,
            confirmLabel: 'Restaurar registro',
            action: () => request('set_voided', target),
            notice: 'Loot restaurado.',
          },
    );
  }
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <Swords size={24} />
          </div>
          <div>
            KALBION<small>YOUR GUILD. IN SYNC.</small>
          </div>
        </div>
        <div className="workspace">
          <span className="workspace-icon">K</span>
          <div>
            Meu workspace<small>Dados locais · Desktop</small>
          </div>
          <Shield size={15} />
        </div>
        <div className="nav-caption">WORKSPACE</div>
        <nav>
          {nav.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setPage(id)}
              className={page === id ? 'selected' : ''}
            >
              <Icon size={19} />
              <span>{label}</span>
              {id !== 'loot' && <small>Em breve</small>}
            </button>
          ))}
        </nav>
        <div className="sidebar-note">
          <FlaskConical size={20} />
          <strong>Ambiente de desenvolvimento</strong>
          <p>Eventos simulados ou manuais. Nenhuma conexão com o jogo.</p>
          <span className="status-dot">Captura desativada</span>
        </div>
        <div className="sidebar-bottom">
          <button
            className={page === 'settings' ? 'selected' : ''}
            onClick={() => setPage('settings')}
          >
            <Settings2 size={18} />
            Configurações
          </button>
          <div className="version">
            <span>Kalbion</span>
            <span>v0.1.0 · dev</span>
          </div>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <div>
            <span>Workspace</span>
            <ChevronRight size={14} />
            {page === 'settings'
              ? 'Configurações'
              : nav.find((item) => item.id === page)?.label}
          </div>
          <span className="local">
            <span /> Armazenamento local
          </span>
        </header>
        <div className="content">
          {error && !modal && !pricing && !confirmation && (
            <div role="alert" className="banner error">
              <span>{error}</span>
              <button
                onClick={() => {
                  setError('');
                  void act(refresh, 'Conexão atualizada.');
                }}
              >
                Tentar novamente
              </button>
            </div>
          )}
          {notice && (
            <div role="status" className="banner success">
              <Check size={16} />
              {notice}
              <button aria-label="Dispensar" onClick={() => setNotice('')}>
                <X size={16} />
              </button>
            </div>
          )}
          {page === 'loot' ? (
            <>
              <div className="page-heading">
                <div>
                  <div className="eyebrow">SEU LOOT, SEM PERDER A CONTA</div>
                  <h1>
                    Loot e sessões<span className="tag">LOCAL</span>
                  </h1>
                  <p>
                    Da primeira coleta ao último acerto. Tudo em um só lugar.
                  </p>
                </div>
                <button
                  className="primary"
                  disabled={!boot || busy}
                  onClick={() => setModal('session')}
                >
                  <Plus size={17} />
                  Nova sessão
                </button>
              </div>
              <div className="simulation-banner">
                <FlaskConical size={19} />
                <div>
                  <strong>Você está no modo de desenvolvimento</strong>
                  <span>
                    Gere eventos de exemplo ou registre loot manualmente. Dados
                    simulados são sempre identificados.
                  </span>
                </div>
                <span className="tag amber">SEM CAPTURA AO VIVO</span>
              </div>
              <section className="session-bar">
                <div className="session-icon">
                  <Archive size={21} />
                </div>
                <div className="session-select">
                  <label htmlFor="session-select">SESSÃO ATUAL</label>
                  <select
                    id="session-select"
                    value={active}
                    disabled={!boot?.sessions.length || busy}
                    onChange={(event) => {
                      setActive(event.target.value);
                      setFilter(emptyFilter);
                    }}
                  >
                    <option value="" disabled>
                      Selecione uma sessão
                    </option>
                    {boot?.sessions.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name}
                        {item.closed_at ? ' · encerrada' : ''}
                      </option>
                    ))}
                  </select>
                </div>
                {session && (
                  <>
                    <span className={`pill ${closed ? '' : 'green'}`}>
                      {closed ? 'Encerrada' : 'Aberta'}
                    </span>
                    <span className="session-meta">
                      {serverNames[session.server]}
                      <i>·</i>
                      {session.city}
                      <small>{date(session.created_at)}</small>
                    </span>
                    <button
                      disabled={busy}
                      onClick={() =>
                        void act(
                          () =>
                            request('set_closed', { ...args, closed: !closed }),
                          closed ? 'Sessão reaberta.' : 'Sessão encerrada.',
                        )
                      }
                    >
                      {closed ? 'Reabrir sessão' : 'Encerrar sessão'}
                    </button>
                  </>
                )}
              </section>
              {!active ? (
                <div className="empty large">
                  <Boxes size={42} />
                  <h2>
                    {loading
                      ? 'Carregando suas sessões…'
                      : 'Toda aventura começa com uma sessão'}
                  </h2>
                  <p>
                    Crie uma sessão para organizar loot, participantes e
                    acertos.
                  </p>
                  {desktop && (
                    <button
                      className="primary"
                      disabled={busy || !boot}
                      onClick={() => setModal('session')}
                    >
                      <Plus size={16} />
                      Criar primeira sessão
                    </button>
                  )}
                </div>
              ) : (
                currentView && (
                  <>
                    <div className="stats">
                      <Stat
                        label="Loot estimado da sessão"
                        value={`${silver(currentView.full_totals.session.estimated_silver)} s`}
                        hint={`${currentView.full_totals.session.unpriced_events} eventos sem preço · não é receita recebida`}
                        accent
                      />
                      <Stat
                        label="Itens registrados"
                        value={silver(currentView.full_totals.session.quantity)}
                        hint={`${currentView.full_totals.session.events} eventos válidos na sessão`}
                      />
                      <Stat
                        label="Participantes"
                        value={String(
                          Object.keys(currentView.full_totals.players).length,
                        )}
                        hint="Jogadores com loot registrado"
                      />
                      <Stat
                        label="Saldo recebido disponível"
                        value={`${silver(currentView.finance.available)} s`}
                        hint="Receitas − despesas − acertos pagos"
                      />
                    </div>
                    <section className="panel">
                      <div className="panel-toolbar">
                        <div className="tabs">
                          <button
                            className={tab === 'events' ? 'active' : ''}
                            onClick={() => setTab('events')}
                          >
                            Registro de loot{' '}
                            <span>
                              {currentView.full_totals.session.events}
                            </span>
                          </button>
                          <button
                            className={tab === 'players' ? 'active' : ''}
                            onClick={() => setTab('players')}
                          >
                            Por jogador
                          </button>
                          <button
                            className={tab === 'ledger' ? 'active' : ''}
                            onClick={() => setTab('ledger')}
                          >
                            Acertos da sessão
                          </button>
                        </div>
                        <div className="toolbar-actions">
                          <button
                            title="Exporta toda a sessão, incluindo acertos"
                            disabled={busy}
                            onClick={() => void exportData('csv')}
                          >
                            <ArrowDownToLine size={15} />
                            CSV
                          </button>
                          <button
                            disabled={busy}
                            onClick={() => void exportData('json')}
                          >
                            JSON
                          </button>
                        </div>
                      </div>
                      {tab !== 'ledger' && (
                        <>
                          <div className="filters">
                            <label className="search">
                              <Search size={16} />
                              <input
                                aria-label="Buscar item"
                                placeholder="Buscar item ou ID…"
                                maxLength={150}
                                value={filter.item}
                                onChange={(event) =>
                                  setFilter({
                                    ...filter,
                                    item: event.target.value,
                                  })
                                }
                              />
                            </label>
                            <input
                              aria-label="Filtrar jogador"
                              placeholder="Todos os jogadores"
                              maxLength={64}
                              value={filter.player}
                              onChange={(event) =>
                                setFilter({
                                  ...filter,
                                  player: event.target.value,
                                })
                              }
                            />
                            {filterSelects.map(({ key, label, options }) => (
                              <select
                                aria-label={key}
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
                                <option value="">{label}</option>
                                {options.map(([value, text]) => (
                                  <option key={value} value={value}>
                                    {text}
                                  </option>
                                ))}
                              </select>
                            ))}
                            <button
                              title="Limpar filtros"
                              onClick={() => setFilter(emptyFilter)}
                            >
                              <X size={15} />
                            </button>
                          </div>
                          <div className="data-actions">
                            <span>
                              {loading
                                ? 'Atualizando…'
                                : `${currentView.rows.length} eventos encontrados`}
                            </span>
                            <div>
                              <button
                                disabled={busy || closed}
                                onClick={() => setModal('import')}
                              >
                                Importar JSON
                              </button>
                              <button
                                disabled={busy || closed}
                                onClick={() => setModal('manual')}
                              >
                                <Plus size={14} />
                                Loot manual
                              </button>
                              <button
                                className="subtle-primary"
                                disabled={busy || closed}
                                onClick={() =>
                                  void act(
                                    () => request('simulate', args),
                                    '7 eventos simulados adicionados.',
                                  )
                                }
                              >
                                <FlaskConical size={14} />
                                Gerar simulação
                              </button>
                            </div>
                          </div>
                        </>
                      )}
                      {tab === 'events' ? (
                        <LootTable
                          rows={currentView.rows}
                          price={setPricing}
                          toggleVoid={toggleVoid}
                          busy={busy}
                          closed={closed}
                        />
                      ) : tab === 'players' ? (
                        <div className="table-wrap">
                          <table>
                            <thead>
                              <tr>
                                <th>Jogador</th>
                                <th>Eventos</th>
                                <th>Quantidade</th>
                                <th>Sem preço</th>
                                <th>Valor estimado</th>
                              </tr>
                            </thead>
                            <tbody>
                              {Object.entries(currentView.totals.players).map(
                                ([player, total]) => (
                                  <tr key={player}>
                                    <td>
                                      <span className="avatar">
                                        {player.slice(0, 1)}
                                      </span>
                                      {player}
                                    </td>
                                    <td>{total.events}</td>
                                    <td>{silver(total.quantity)}</td>
                                    <td>{total.unpriced_events}</td>
                                    <td className="silver">
                                      {silver(total.estimated_silver)} s
                                    </td>
                                  </tr>
                                ),
                              )}
                            </tbody>
                          </table>
                          {!Object.keys(currentView.totals.players).length && (
                            <div className="empty">
                              Nenhum jogador corresponde aos filtros.
                            </div>
                          )}
                        </div>
                      ) : (
                        <LedgerPanel
                          ledger={currentView.ledger}
                          finance={currentView.finance}
                          busy={busy}
                          add={() => setModal('ledger')}
                          split={() => setModal('split')}
                          reverse={(entry) =>
                            setConfirmation({
                              title: 'Estornar lançamento',
                              message: `${entry.description} · ${silver(entry.amount)} s. Um lançamento oposto é registrado; o original continua no histórico. Estornos não podem ser desfeitos.`,
                              confirmLabel: 'Registrar estorno',
                              action: () =>
                                request('reverse_ledger', {
                                  ...args,
                                  entry_id: entry.id,
                                }),
                              notice: 'Estorno registrado.',
                            })
                          }
                        />
                      )}
                      <footer className="table-footer">
                        <span>
                          {tab === 'ledger'
                            ? 'Lançamentos manuais · valores inteiros em silver'
                            : `Total filtrado: ${silver(currentView.totals.session.quantity)} itens · ${currentView.totals.session.unpriced_events} eventos sem preço · anulados não contam`}
                        </span>
                        <strong>
                          {tab === 'ledger'
                            ? `Disponível: ${silver(currentView.finance.available)} s`
                            : `${silver(currentView.totals.session.estimated_silver)} s estimados`}
                        </strong>
                      </footer>
                    </section>
                    <div className="footnote">
                      <Shield size={14} />
                      Dados salvos no dispositivo. Preços manuais são
                      estimativas no contexto da sessão.
                    </div>
                  </>
                )
              )}
              {active && !currentView && (
                <div className="empty">
                  {loading
                    ? 'Carregando sessão…'
                    : 'Não foi possível carregar esta sessão.'}
                </div>
              )}
            </>
          ) : page === 'settings' ? (
            <>
              <div className="page-heading">
                <div>
                  <div className="eyebrow">DO SEU JEITO</div>
                  <h1>Configurações</h1>
                  <p>Preferências locais, catálogo e status das integrações.</p>
                </div>
              </div>
              {boot && (
                <SettingsPage
                  boot={boot}
                  busy={busy}
                  save={(settings) =>
                    void act(() => request('settings', { settings }))
                  }
                  importCatalog={() =>
                    void act(async () => {
                      const info = await importCatalog();
                      if (!info) return 'Importação do catálogo cancelada.';
                      return `Catálogo importado: ${silver(info.item_count)} itens${info.skipped_count ? `, ${silver(info.skipped_count)} inválidos ignorados` : ''}.`;
                    })
                  }
                />
              )}
            </>
          ) : (
            <>
              <div className="page-heading">
                <div>
                  <div className="eyebrow">PRÓXIMOS CAPÍTULOS</div>
                  <h1>{nav.find((item) => item.id === page)?.label}</h1>
                </div>
                <span className="tag">NÃO IMPLEMENTADO</span>
              </div>
              <section className="empty large future">
                <LayoutDashboard size={44} />
                <h2>Um espaço reservado para o próximo passo.</h2>
                <p>
                  {page === 'crafting'
                    ? 'Receitas, materiais, taxas, retorno de recursos e margem de produção serão implementados em uma próxima entrega.'
                    : page === 'finance'
                      ? 'O financeiro consolidado ainda não está disponível. Receitas, despesas, regear e divisão já podem ser registrados em “Acertos da sessão”.'
                      : 'Presets de clap, press, brawl e gank, com builds e substitutos cadastrados manualmente, estão planejados.'}
                </p>
                {page === 'finance' && (
                  <button
                    onClick={() => {
                      setPage('loot');
                      setTab('ledger');
                    }}
                  >
                    Ir para acertos da sessão
                    <ChevronRight size={16} />
                  </button>
                )}
              </section>
            </>
          )}
        </div>
      </main>
      {modal && (
        <Dialog
          title={
            {
              session: 'Nova sessão',
              manual: 'Registrar loot manual',
              import: 'Importar eventos JSON',
              ledger: 'Lançamento financeiro',
              split: 'Divisão igualitária',
            }[modal]
          }
          close={() => {
            if (!busy) setModal(null);
          }}
        >
          {error && (
            <div role="alert" className="banner error">
              {error}
            </div>
          )}
          {modal === 'session' && (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                const name = String(
                  new FormData(event.currentTarget).get('name'),
                );
                void act(async () => {
                  const created = await request<Session>('create_session', {
                    name,
                  });
                  setActive(created.id);
                  setFilter(emptyFilter);
                }, 'Sessão criada.');
              }}
            >
              <Field label="Nome da sessão">
                <input
                  autoFocus
                  name="name"
                  required
                  maxLength={120}
                  placeholder="Ex.: Roads com a guilda"
                />
              </Field>
              <p className="help">
                {serverNames[boot?.settings.server ?? 'americas']} ·{' '}
                {boot?.settings.city}. O contexto de preços fica fixo nesta
                sessão.
              </p>
              <button className="primary" disabled={busy}>
                Criar sessão
              </button>
            </form>
          )}
          {modal === 'manual' && (
            <ManualForm
              busy={busy}
              submit={(data) =>
                void act(
                  () => request('manual', { ...args, ...data }),
                  'Loot manual registrado.',
                )
              }
            />
          )}
          {modal === 'import' && (
            <ImportForm
              sessionId={active}
              busy={busy}
              submit={(json) =>
                void act(async () => {
                  const result = await request<InsertResult>('import', {
                    ...args,
                    json,
                  });
                  return `${result.inserted} inseridos; ${result.duplicates} duplicados ignorados.`;
                })
              }
            />
          )}
          {modal === 'ledger' && (
            <LedgerForm
              busy={busy}
              submit={(data) =>
                void act(() => request('ledger', { ...args, ...data }))
              }
            />
          )}
          {modal === 'split' && currentView && (
            <SplitForm
              available={currentView.finance.available}
              players={Object.keys(currentView.full_totals.players)}
              busy={busy}
              preview={(amount, players) =>
                request<Share[]>('split', {
                  ...args,
                  amount,
                  players,
                  confirm: false,
                })
              }
              submit={(amount, players) =>
                void act(
                  () =>
                    request('split', {
                      ...args,
                      amount,
                      players,
                      confirm: true,
                    }),
                  'Divisão registrada como acertos pagos.',
                )
              }
            />
          )}
        </Dialog>
      )}
      {pricing && (
        <Dialog
          title="Preço manual por unidade"
          close={() => {
            if (!busy) setPricing(null);
          }}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              const value = new FormData(event.currentTarget).get('amount');
              void act(
                () =>
                  request('price', {
                    ...args,
                    item_id: pricing.event.item.id,
                    quality: pricing.event.quality,
                    amount: Number(value),
                  }),
                'Preço atualizado para este item e qualidade na sessão.',
              );
            }}
          >
            {error && (
              <p role="alert" className="error">
                {error}
              </p>
            )}
            <p>
              {pricing.event.item.name} · {qualityLabel(pricing.event.quality)}
            </p>
            <Field label="Silver por unidade (zero é um preço conhecido)">
              <input
                autoFocus
                name="amount"
                type="number"
                min="0"
                step="1"
                max="1000000000000"
                defaultValue={pricing.price?.unit_silver}
                required
              />
            </Field>
            <p className="help">
              {serverNames[session?.server ?? 'americas']} · {session?.city}.
              Aplica a todos os eventos deste item e qualidade na sessão
              {pricing.event.quality === null
                ? ', separado das qualidades conhecidas'
                : ''}
              .
            </p>
            <div className="form-actions">
              <button
                type="button"
                disabled={busy || !pricing.price}
                onClick={() =>
                  void act(() =>
                    request('price', {
                      ...args,
                      item_id: pricing.event.item.id,
                      quality: pricing.event.quality,
                      amount: null,
                    }),
                  )
                }
              >
                Remover preço
              </button>
              <button className="primary" disabled={busy}>
                Salvar estimativa
              </button>
            </div>
          </form>
        </Dialog>
      )}
      {confirmation && (
        <ConfirmDialog
          confirmation={confirmation}
          error={error}
          busy={busy}
          cancel={() => {
            if (!busy) setConfirmation(null);
          }}
          confirm={() => void act(confirmation.action, confirmation.notice)}
        />
      )}
    </div>
  );
}

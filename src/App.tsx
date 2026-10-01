import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from 'react';
import {
  Archive,
  ArrowDownToLine,
  ArrowUpRight,
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
import { desktop, exportSession, request } from './api';
import type {
  Bootstrap,
  Filter,
  Item,
  LootRow,
  Session,
  Settings,
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
const qualities = ['Normal', 'Bom', 'Excepcional', 'Excelente', 'Obra-prima'];
const serverNames: Record<string, string> = {
  americas: 'Américas',
  europe: 'Europa',
  asia: 'Ásia',
};
const kinds: Record<string, string> = {
  income: 'Receita recebida',
  expense: 'Despesa',
  regear: 'Regear',
  settlement: 'Acerto pago',
};
const silver = (amount: number) =>
  new Intl.NumberFormat('pt-BR').format(amount);
const date = (value: string) =>
  new Date(value).toLocaleString('pt-BR', {
    dateStyle: 'short',
    timeStyle: 'short',
  });
type Page = 'loot' | 'crafting' | 'finance' | 'compositions' | 'settings';
type Modal = 'session' | 'manual' | 'import' | 'ledger' | 'split' | null;

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}
function Stat({
  label,
  value,
  hint,
  accent = false,
}: {
  label: string;
  value: string;
  hint: string;
  accent?: boolean;
}) {
  return (
    <div className={`stat ${accent ? 'accent' : ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{hint}</small>
    </div>
  );
}
function Dialog({
  title,
  close,
  children,
}: {
  title: string;
  close: () => void;
  children: ReactNode;
}) {
  const container = useRef<HTMLElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const panel = container.current;
    if (!panel) return;
    if (!panel.contains(document.activeElement)) {
      panel
        .querySelector<HTMLElement>('input, select, textarea, button')
        ?.focus();
    }
    const trap = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const elements = Array.from(
        panel.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled)',
        ),
      );
      const first = elements[0];
      const last = elements[elements.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    panel.addEventListener('keydown', trap);
    return () => {
      panel.removeEventListener('keydown', trap);
      previous?.focus();
    };
  }, []);
  return (
    <div className="overlay" onClick={close}>
      <section
        ref={container}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="dialog"
        onClick={(event) => event.stopPropagation()}
      >
        <header>
          <h2>{title}</h2>
          <button aria-label="Fechar" onClick={close}>
            <X size={18} />
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}

export default function App() {
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [active, setActive] = useState('');
  const [view, setView] = useState<View | null>(null);
  const [page, setPage] = useState<Page>('loot');
  const [filter, setFilter] = useState<Filter>(emptyFilter);
  const [tab, setTab] = useState<'events' | 'players' | 'ledger'>('events');
  const [modal, setModal] = useState<Modal>(null);
  const [pricing, setPricing] = useState<LootRow | null>(null);
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
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busy) {
        setModal(null);
        setPricing(null);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [busy]);
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
      setModal(null);
      setPricing(null);
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
  const nav = [
    { id: 'loot', label: 'Loot e sessões', icon: Boxes },
    { id: 'crafting', label: 'Crafting', icon: Hammer },
    { id: 'finance', label: 'Financeiro', icon: CircleDollarSign },
    { id: 'compositions', label: 'Composições', icon: Users },
  ] as const;
  async function exportData(format: string) {
    if (mutation.current) return;
    mutation.current = true;
    setBusy(true);
    setError('');
    try {
      const saved = await exportSession(active, format);
      setNotice(
        saved
          ? `Sessão completa exportada em ${format.toUpperCase()}.`
          : 'Exportação cancelada.',
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      mutation.current = false;
      setBusy(false);
    }
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
          {error && (
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
                    <span
                      className={`pill ${session.closed_at ? '' : 'green'}`}
                    >
                      {session.closed_at ? 'Encerrada' : 'Aberta'}
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
                            request('set_closed', {
                              ...args,
                              closed: !session.closed_at,
                            }),
                          session.closed_at
                            ? 'Sessão reaberta.'
                            : 'Sessão encerrada.',
                        )
                      }
                    >
                      {session.closed_at ? 'Reabrir sessão' : 'Encerrar sessão'}
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
                        hint={`${currentView.full_totals.session.events} eventos na sessão`}
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
                              value={filter.player}
                              onChange={(event) =>
                                setFilter({
                                  ...filter,
                                  player: event.target.value,
                                })
                              }
                            />
                            {(['tier', 'enchantment', 'quality'] as const).map(
                              (key) => (
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
                                  <option value="">
                                    {key === 'tier'
                                      ? 'Todos os tiers'
                                      : key === 'enchantment'
                                        ? 'Encantamento'
                                        : 'Qualidade'}
                                  </option>
                                  {(key === 'tier'
                                    ? [1, 2, 3, 4, 5, 6, 7, 8]
                                    : key === 'enchantment'
                                      ? [0, 1, 2, 3, 4]
                                      : [1, 2, 3, 4, 5]
                                  ).map((value) => (
                                    <option key={value} value={value}>
                                      {key === 'tier'
                                        ? `T${value}`
                                        : key === 'enchantment'
                                          ? `.${value}`
                                          : qualities[value - 1]}
                                    </option>
                                  ))}
                                </select>
                              ),
                            )}
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
                                disabled={busy || !!session?.closed_at}
                                onClick={() => setModal('import')}
                              >
                                Importar JSON
                              </button>
                              <button
                                disabled={busy || !!session?.closed_at}
                                onClick={() => setModal('manual')}
                              >
                                <Plus size={14} />
                                Loot manual
                              </button>
                              <button
                                className="subtle-primary"
                                disabled={busy || !!session?.closed_at}
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
                      {loading ? (
                        <div className="empty" role="status">
                          Carregando registros…
                        </div>
                      ) : tab === 'events' ? (
                        <LootTable
                          rows={currentView.rows}
                          price={setPricing}
                          busy={busy}
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
                          {!currentView.rows.length && (
                            <div className="empty">
                              Nenhum jogador corresponde aos filtros.
                            </div>
                          )}
                        </div>
                      ) : (
                        <>
                          <div className="ledger-summary">
                            <div>
                              <span>Receitas recebidas</span>
                              <strong>
                                {silver(currentView.finance.income)} s
                              </strong>
                            </div>
                            <div>
                              <span>Despesas e regear</span>
                              <strong>
                                {silver(currentView.finance.expenses)} s
                              </strong>
                            </div>
                            <div>
                              <span>Acertos pagos</span>
                              <strong>
                                {silver(currentView.finance.settlements)} s
                              </strong>
                            </div>
                            <button
                              disabled={busy}
                              onClick={() => setModal('ledger')}
                            >
                              <Plus size={15} />
                              Lançamento
                            </button>
                            <button
                              className="primary"
                              disabled={
                                busy || currentView.finance.available <= 0
                              }
                              onClick={() => setModal('split')}
                            >
                              Dividir saldo
                              <ArrowUpRight size={15} />
                            </button>
                          </div>
                          <div className="table-wrap">
                            <table>
                              <thead>
                                <tr>
                                  <th>Horário</th>
                                  <th>Tipo</th>
                                  <th>Jogador</th>
                                  <th>Descrição</th>
                                  <th>Silver</th>
                                </tr>
                              </thead>
                              <tbody>
                                {currentView.ledger.map((entry) => (
                                  <tr key={entry.id}>
                                    <td>{date(entry.occurred_at)}</td>
                                    <td>
                                      <span className="pill">
                                        {kinds[entry.kind]}
                                      </span>
                                    </td>
                                    <td>{entry.player}</td>
                                    <td>{entry.description}</td>
                                    <td>{silver(entry.amount)}</td>
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                            {!currentView.ledger.length && (
                              <div className="empty">
                                <CircleDollarSign size={30} />
                                <h3>Nenhum lançamento financeiro</h3>
                                <p>
                                  Registre vendas recebidas, despesas e regear.
                                  Loot estimado não entra no saldo.
                                </p>
                              </div>
                            )}
                          </div>
                        </>
                      )}
                      <footer className="table-footer">
                        <span>
                          {tab === 'ledger'
                            ? 'Lançamentos manuais · valores inteiros em silver'
                            : `Total filtrado: ${silver(currentView.totals.session.quantity)} itens · ${currentView.totals.session.unpriced_events} eventos sem preço`}
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
                  <p>Preferências locais e status das integrações.</p>
                </div>
              </div>
              {boot && (
                <SettingsPanel
                  settings={boot.settings}
                  busy={busy}
                  save={(settings) =>
                    void act(() => request('settings', { settings }))
                  }
                />
              )}
              <section className="settings-card">
                <Shield />
                <h2>Licenciamento</h2>
                <span className="pill">Desabilitado · desenvolvimento</span>
                <p>{boot?.license.reason ?? 'Nenhuma licença validada.'}</p>
                <p>
                  Esta versão funciona localmente sem autenticação. Não há
                  validação offline de licença. Histórico e exportações
                  permanecerão acessíveis após expiração na integração futura.
                </p>
              </section>
              <section className="settings-card">
                <FlaskConical />
                <h2>Integrações</h2>
                <p>
                  Albion Data Project: contrato preparado; consulta ainda não
                  implementada. Preços manuais disponíveis.
                </p>
                <p>
                  Captura de rede e OCR: ausentes. Nenhum privilégio
                  administrativo é necessário.
                </p>
              </section>
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
              catalog={boot?.catalog ?? []}
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
                  const result = await request<{
                    inserted: number;
                    duplicates: number;
                  }>('import', { ...args, json });
                  return `${result.inserted} inseridos; ${result.duplicates} duplicados ignorados.`;
                }, 'Importação concluída. Replays idênticos foram ignorados.')
              }
            />
          )}
          {modal === 'ledger' && (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                const data = new FormData(event.currentTarget);
                void act(() =>
                  request('ledger', {
                    ...args,
                    kind: data.get('kind'),
                    player: data.get('player'),
                    description: data.get('description'),
                    amount: Number(data.get('amount')),
                  }),
                );
              }}
            >
              <Field label="Tipo">
                <select name="kind">
                  {Object.entries(kinds).map(([key, label]) => (
                    <option key={key} value={key}>
                      {label}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label="Jogador / responsável">
                <input name="player" required maxLength={64} />
              </Field>
              <Field label="Descrição">
                <input name="description" required maxLength={200} />
              </Field>
              <Field label="Valor efetivo em silver">
                <input
                  name="amount"
                  type="number"
                  min="1"
                  max="1000000000000"
                  step="1"
                  required
                />
              </Field>
              <p className="help">
                Acerto pago reduz o saldo do caixa da sessão. Registre apenas
                valores efetivamente recebidos ou pagos.
              </p>
              <button className="primary" disabled={busy}>
                Registrar lançamento
              </button>
            </form>
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
                    quality: pricing.event.item.quality,
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
              {pricing.event.item.name} ·{' '}
              {qualities[pricing.event.item.quality - 1]}
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
              Aplica a todos os eventos deste item e qualidade na sessão.
            </p>
            <div className="form-actions">
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void act(() =>
                    request('price', {
                      ...args,
                      item_id: pricing.event.item.id,
                      quality: pricing.event.item.quality,
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
    </div>
  );
}

function LootTable({
  rows,
  price,
  busy,
}: {
  rows: LootRow[];
  price: (row: LootRow) => void;
  busy: boolean;
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
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={`${row.event.source}:${row.event.id}`}>
              <td>
                <div className="item-cell">
                  <span className={`item-icon tier-${row.event.item.tier}`}>
                    <Boxes size={20} />
                    <small>T{row.event.item.tier}</small>
                  </span>
                  <div>
                    <strong>{row.event.item.name}</strong>
                    <small>{row.event.item.id}</small>
                  </div>
                </div>
              </td>
              <td>
                <span className="avatar">{row.event.player.slice(0, 1)}</span>
                {row.event.player}
              </td>
              <td>
                <span className="tier">T{row.event.item.tier}</span>
                <span
                  className={`enchantment enchantment-${row.event.item.enchantment}`}
                >
                  .{row.event.item.enchantment}
                </span>
              </td>
              <td>{qualities[row.event.item.quality - 1]}</td>
              <td className="numeric">{silver(row.event.quantity)}</td>
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
                    title={`${row.price.server} · ${row.price.city} · ${date(row.price.queried_at)}`}
                  >
                    Manual · {row.price.city}
                    <br />
                    {date(row.price.queried_at)}
                  </small>
                )}
              </td>
              <td>
                <small>{date(row.event.occurred_at)}</small>
                <span
                  className={`origin ${row.event.origin === 'simulated' ? 'simulated' : ''}`}
                >
                  {row.event.origin === 'simulated'
                    ? 'Simulado'
                    : row.event.origin === 'manual'
                      ? 'Manual'
                      : 'Observado (declarado)'}
                  {row.imported ? ' · Importado' : ''}
                </span>
              </td>
            </tr>
          ))}
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

function ManualForm({
  catalog,
  busy,
  submit,
}: {
  catalog: Item[];
  busy: boolean;
  submit: (data: { item: Item; player: string; quantity: number }) => void;
}) {
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        const data = new FormData(event.currentTarget);
        const item = catalog[Number(data.get('item'))];
        submit({
          item: { ...item, quality: Number(data.get('quality')) },
          player: String(data.get('player')),
          quantity: Number(data.get('quantity')),
        });
      }}
    >
      <Field label="Jogador">
        <input autoFocus name="player" required maxLength={64} />
      </Field>
      <Field label="Item do catálogo inicial">
        <select name="item">
          {catalog.map((item, index) => (
            <option key={item.id} value={index}>
              {item.name} · T{item.tier}.{item.enchantment}
            </option>
          ))}
        </select>
      </Field>
      <div className="form-grid">
        <Field label="Qualidade">
          <select name="quality">
            {qualities.map((quality, index) => (
              <option key={quality} value={index + 1}>
                {quality}
              </option>
            ))}
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
        Origem manual. Horário registrado no momento do envio.
      </p>
      <button className="primary" disabled={busy}>
        Registrar loot
      </button>
    </form>
  );
}
function ImportForm({
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
        Contrato v1: schema_version e events. Máximo 5 MB / 10.000 eventos. A
        origem é declarada pelo arquivo e não é certificada. O lote inteiro é
        validado antes de salvar.
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
          placeholder={'{"schema_version":1,"events":[…]}'}
        />
      </Field>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <p className="help">
        session_id deve corresponder ao destino. Exports da mesma sessão podem
        ser reimportados; preços e acertos não são restaurados por esta
        importação.
      </p>
      <button className="primary" disabled={busy || !json}>
        Validar e importar
      </button>
    </form>
  );
}
function SplitForm({
  available,
  players: initialPlayers,
  busy,
  preview,
  submit,
}: {
  available: number;
  players: string[];
  busy: boolean;
  preview: (amount: number, players: string[]) => Promise<Share[]>;
  submit: (amount: number, players: string[]) => void;
}) {
  const [players, setPlayers] = useState(initialPlayers.join('\n'));
  const [amount, setAmount] = useState(Math.min(available, 1_000_000_000_000));
  const [shares, setShares] = useState<Share[]>([]);
  const [error, setError] = useState('');
  const [pending, setPending] = useState(false);
  const names = () =>
    players
      .split('\n')
      .map((player) => player.trim())
      .filter(Boolean);
  async function calculate(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError('');
    try {
      setShares(await preview(amount, names()));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPending(false);
    }
  }
  return (
    <form onSubmit={calculate}>
      <Field label={`Saldo a dividir (disponível: ${silver(available)} s)`}>
        <input
          disabled={pending}
          type="number"
          value={amount}
          min="1"
          max={Math.min(available, 1_000_000_000_000)}
          step="1"
          onChange={(event) => {
            setAmount(Number(event.target.value));
            setShares([]);
          }}
          required
        />
      </Field>
      <Field label="Participantes, um por linha">
        <textarea
          disabled={pending}
          rows={4}
          value={players}
          onChange={(event) => {
            setPlayers(event.target.value);
            setShares([]);
          }}
          required
        />
      </Field>
      <p className="help">
        Divisão em silver inteiro. O resto é distribuído em ordem alfabética.
        Confirmar registra pagamentos efetivos aos participantes.
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {shares.map((share) => (
        <div className="share" key={share.player}>
          <span>{share.player}</span>
          <strong>{silver(share.silver)} s</strong>
        </div>
      ))}
      <div className="form-actions">
        <button disabled={busy || pending}>Calcular divisão</button>
        {!!shares.length && (
          <button
            type="button"
            className="primary"
            disabled={busy || pending}
            onClick={() => submit(amount, names())}
          >
            Confirmar pagamentos
          </button>
        )}
      </div>
    </form>
  );
}
function SettingsPanel({
  settings,
  busy,
  save,
}: {
  settings: Settings;
  busy: boolean;
  save: (settings: Settings) => void;
}) {
  return (
    <section className="settings-card">
      <h2>Mercado padrão</h2>
      <form
        key={`${settings.server}:${settings.city}`}
        onSubmit={(event) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          save({
            server: String(data.get('server')),
            city: String(data.get('city')),
          });
        }}
      >
        <div className="form-grid">
          <Field label="Servidor / região">
            <select name="server" defaultValue={settings.server}>
              {Object.entries(serverNames).map(([key, label]) => (
                <option value={key} key={key}>
                  {label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Cidade">
            <select name="city" defaultValue={settings.city}>
              {[
                'Bridgewatch',
                'Martlock',
                'Lymhurst',
                'Fort Sterling',
                'Thetford',
                'Caerleon',
                'Brecilien',
              ].map((city) => (
                <option key={city}>{city}</option>
              ))}
            </select>
          </Field>
        </div>
        <p className="help">
          Aplica somente a novas sessões, preservando o contexto dos preços
          existentes.
        </p>
        <button className="primary" disabled={busy}>
          Salvar preferências
        </button>
      </form>
    </section>
  );
}

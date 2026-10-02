import { useCallback, useEffect, useRef, useState } from 'react';
import { Archive, Plus } from 'lucide-react';
import {
  desktop,
  exportSession,
  importCatalog,
  importCapture,
  liveCaptureStatus,
  refreshMarketPrices,
  request,
  startLiveCapture,
  stopLiveCapture,
} from './api';
import { CaptureForm, captureSummary } from './capture';
import {
  ConfirmDialog,
  Dialog,
  EmptyState,
  Field,
  InlineError,
  Notices,
  type Confirmation,
} from './components';
import { LedgerForm, LedgerView, SplitForm } from './finance';
import {
  age,
  marketSummary,
  qualityLabel,
  serverNames,
  silver,
} from './format';
import {
  emptyFilter,
  ImportForm,
  LootView,
  ManualForm,
  PlayersView,
} from './loot';
import { LiveCaptureForm, LivePanel } from './live';
import { FuturePage, SettingsPage } from './settings';
import {
  SessionHeader,
  Sidebar,
  Summary,
  viewTitles,
  type ViewId,
} from './shell';
import type {
  Bootstrap,
  Filter,
  InsertResult,
  LiveStatus,
  LootRow,
  Session,
  Share,
  View,
} from './types';

type Modal =
  | 'session'
  | 'manual'
  | 'import'
  | 'capture'
  | 'live'
  | 'ledger'
  | 'split'
  | null;
/** How often the live capture status is read while a capture runs. */
const LIVE_POLL_MS = 1000;
const sessionViews: ViewId[] = ['loot', 'players', 'ledger'];

export default function App() {
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [active, setActive] = useState('');
  const [view, setView] = useState<View | null>(null);
  const [page, setPage] = useState<ViewId>('loot');
  const [filter, setFilter] = useState<Filter>(emptyFilter);
  const [modal, setModal] = useState<Modal>(null);
  const [pricing, setPricing] = useState<LootRow | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [revision, setRevision] = useState(0);
  const [live, setLive] = useState<LiveStatus | null>(null);
  const liveInserted = useRef(0);
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
    if (!desktop) return;
    liveCaptureStatus()
      .then(setLive)
      .catch(() => setLive(null));
  }, []);
  const liveRunning = live?.state === 'running';
  useEffect(() => {
    if (!liveRunning) return;
    const timer = setInterval(() => {
      liveCaptureStatus()
        .then((status) => {
          setLive(status);
          // New loot was stored: reload the session view and totals.
          if (status.inserted !== liveInserted.current) {
            liveInserted.current = status.inserted;
            setRevision((value) => value + 1);
          }
        })
        .catch((reason) => setError(String(reason)));
    }, LIVE_POLL_MS);
    return () => clearInterval(timer);
  }, [liveRunning]);
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
  const dialogOpen = !!(modal || pricing || confirmation);
  const isSessionView = sessionViews.includes(page);

  function exportAs(format: 'csv' | 'json') {
    void act(async () => {
      const saved = await exportSession(active, format);
      return saved
        ? `Sessão exportada em ${format.toUpperCase()}.`
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
    const what = `${row.event.item.name} × ${silver(row.event.quantity)}, de ${row.event.player}.`;
    setConfirmation(
      voided
        ? {
            title: 'Anular loot',
            message: `${what} O registro continua no histórico e na exportação, mas deixa de contar nos totais. Você pode restaurá-lo depois.`,
            confirmLabel: 'Anular registro',
            action: () => request('set_voided', target),
            notice: 'Loot anulado; totais atualizados.',
          }
        : {
            title: 'Restaurar loot',
            message: `${what} O registro volta a contar nos totais.`,
            confirmLabel: 'Restaurar registro',
            action: () => request('set_voided', target),
            notice: 'Loot restaurado.',
          },
    );
  }

  function sessionContent() {
    if (!boot) return null;
    if (!active) {
      return (
        <EmptyState
          icon={<Archive size={32} aria-hidden />}
          title={loading ? 'Carregando sessões…' : 'Nenhuma sessão ainda'}
          action={
            desktop && (
              <button
                className="primary"
                disabled={busy}
                onClick={() => setModal('session')}
              >
                <Plus size={16} aria-hidden />
                Nova sessão
              </button>
            )
          }
        >
          Uma sessão agrupa o loot, os participantes e os acertos de uma
          atividade, como uma dungeon ou um ZvZ.
        </EmptyState>
      );
    }
    if (!currentView) {
      return (
        <p className="status-line" role="status">
          {loading
            ? 'Carregando sessão…'
            : 'Não foi possível carregar esta sessão.'}
        </p>
      );
    }
    if (page === 'players') return <PlayersView view={currentView} />;
    if (page === 'ledger') {
      return (
        <LedgerView
          ledger={currentView.ledger}
          finance={currentView.finance}
          busy={busy}
          add={() => setModal('ledger')}
          split={() => setModal('split')}
          reverse={(entry) =>
            setConfirmation({
              title: 'Estornar lançamento',
              message: `${entry.description}, ${silver(entry.amount)} s. Um lançamento oposto é registrado e o original continua no histórico. Um estorno não pode ser desfeito.`,
              confirmLabel: 'Registrar estorno',
              action: () =>
                request('reverse_ledger', { ...args, entry_id: entry.id }),
              notice: 'Estorno registrado.',
            })
          }
        />
      );
    }
    return (
      <LootView
        view={currentView}
        filter={filter}
        setFilter={setFilter}
        loading={loading}
        busy={busy}
        closed={closed}
        simulate={() =>
          void act(
            () => request('simulate', args),
            '7 eventos simulados adicionados.',
          )
        }
        register={() => setModal('manual')}
        refreshPrices={() =>
          void act(async () => marketSummary(await refreshMarketPrices(active)))
        }
        price={setPricing}
        toggleVoid={toggleVoid}
      />
    );
  }

  return (
    <>
      <div className="app-shell" inert={dialogOpen}>
        <Sidebar view={page} go={setPage} data={currentView} />
        <main>
          {isSessionView && (
            <SessionHeader
              sessions={boot?.sessions ?? []}
              session={session}
              busy={busy || !boot}
              select={(id) => {
                setActive(id);
                setFilter(emptyFilter);
              }}
              create={() => setModal('session')}
              toggleClosed={() =>
                void act(
                  () => request('set_closed', { ...args, closed: !closed }),
                  closed ? 'Sessão reaberta.' : 'Sessão encerrada.',
                )
              }
              exportAs={exportAs}
              openImport={() => setModal('import')}
              openCapture={() => setModal('capture')}
              openLive={() => setModal('live')}
              capturing={liveRunning}
            />
          )}
          {isSessionView && currentView && <Summary view={currentView} />}
          {isSessionView && live && live.session_id === active && (
            <LivePanel
              status={live}
              busy={busy}
              stop={() =>
                void act(async () => {
                  const status = await stopLiveCapture();
                  setLive(status);
                  return `Captura parada: ${status.inserted} loots gravados.`;
                })
              }
            />
          )}
          <div
            className={`content ${isSessionView && page === 'loot' ? '' : 'scroll'}`}
          >
            <h1 className={isSessionView ? 'visually-hidden' : 'page-title'}>
              {viewTitles[page]}
            </h1>
            {!dialogOpen && (
              <Notices
                error={error}
                notice={notice}
                retry={() => {
                  setError('');
                  void act(refresh, 'Dados recarregados.');
                }}
                dismissNotice={() => setNotice('')}
              />
            )}
            {isSessionView ? (
              sessionContent()
            ) : page === 'settings' ? (
              boot && (
                <SettingsPage
                  boot={boot}
                  busy={busy}
                  save={(settings) =>
                    void act(
                      () => request('settings', { settings }),
                      'Preferências salvas.',
                    )
                  }
                  importCatalog={() =>
                    void act(async () => {
                      const info = await importCatalog();
                      if (!info) return 'Importação do catálogo cancelada.';
                      return `Catálogo importado: ${silver(info.item_count)} itens${info.skipped_count ? `, ${silver(info.skipped_count)} inválidos ignorados` : ''}.`;
                    })
                  }
                />
              )
            ) : (
              <FuturePage view={page} go={setPage} />
            )}
          </div>
        </main>
      </div>
      {modal && (
        <Dialog
          title={
            {
              session: 'Nova sessão',
              manual: 'Registrar loot',
              import: 'Importar JSON',
              capture: 'Importar captura de loot',
              live: 'Captura ao vivo',
              ledger: 'Novo lançamento',
              split: 'Dividir saldo',
            }[modal]
          }
          close={() => {
            if (!busy) setModal(null);
          }}
        >
          <InlineError message={error} />
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
                  setPage('loot');
                }, 'Sessão criada.');
              }}
            >
              <Field label="Nome da sessão">
                <input
                  autoFocus
                  name="name"
                  required
                  maxLength={120}
                  placeholder="Por exemplo: Roads com a guilda"
                />
              </Field>
              <p className="help">
                Contexto de preço:{' '}
                {serverNames[boot?.settings.server ?? 'americas']},{' '}
                {boot?.settings.city}. Ele fica fixo nesta sessão; altere o
                padrão em Configurações.
              </p>
              <div className="form-actions">
                <button className="primary" disabled={busy}>
                  Criar sessão
                </button>
              </div>
            </form>
          )}
          {modal === 'manual' && (
            <ManualForm
              busy={busy}
              submit={(data) =>
                void act(
                  () => request('manual', { ...args, ...data }),
                  'Loot registrado.',
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
          {modal === 'capture' && (
            <CaptureForm
              players={Object.keys(currentView?.full_totals.players ?? {})}
              busy={busy}
              submit={(roster) =>
                void act(async () => {
                  const result = await importCapture(active, roster);
                  return result
                    ? captureSummary(result)
                    : 'Importação de captura cancelada.';
                })
              }
            />
          )}
          {modal === 'live' && (
            <LiveCaptureForm
              players={Object.keys(currentView?.full_totals.players ?? {})}
              busy={busy}
              submit={(roster, networkInterface, trace) =>
                void act(async () => {
                  const status = await startLiveCapture(
                    active,
                    roster,
                    networkInterface,
                    trace,
                  );
                  liveInserted.current = status.inserted;
                  setLive(status);
                  return `Captura iniciada em ${networkInterface}.`;
                })
              }
            />
          )}
          {modal === 'ledger' && (
            <LedgerForm
              busy={busy}
              submit={(data) =>
                void act(
                  () => request('ledger', { ...args, ...data }),
                  'Lançamento registrado.',
                )
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
                'Preço salvo para este item e qualidade na sessão.',
              );
            }}
          >
            <InlineError message={error} />
            <p>
              <strong>{pricing.event.item.name}</strong>, qualidade{' '}
              {qualityLabel(
                pricing.event.quality,
                pricing.event.item.has_quality,
              ).toLowerCase()}
            </p>
            {pricing.price?.source === 'albion_data' &&
              pricing.price.observed_at && (
                <p className="help">
                  Preço atual: {silver(pricing.price.unit_silver)} s, menor
                  oferta de venda no Albion Data Project, observada{' '}
                  {age(pricing.price.observed_at)}. Salvar aqui cria um preço
                  manual, que prevalece sobre o mercado; remover o preço deixa a
                  próxima atualização preenchê-lo de novo.
                </p>
              )}
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
              Estimativa no mercado de {session?.city} (
              {serverNames[session?.server ?? 'americas']}). Vale para todos os
              registros deste item
              {!pricing.event.item.has_quality
                ? ' na sessão'
                : pricing.event.quality === null
                  ? ' com qualidade desconhecida na sessão, separada das qualidades conhecidas. Sem qualidade informada não há cotação de mercado'
                  : ' com esta qualidade na sessão'}
              .
            </p>
            <div className="form-actions">
              <button
                type="button"
                disabled={busy || !pricing.price}
                onClick={() =>
                  void act(
                    () =>
                      request('price', {
                        ...args,
                        item_id: pricing.event.item.id,
                        quality: pricing.event.quality,
                        amount: null,
                      }),
                    'Preço removido.',
                  )
                }
              >
                Remover preço
              </button>
              <button className="primary" disabled={busy}>
                Salvar preço
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
    </>
  );
}

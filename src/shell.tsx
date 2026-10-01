import type { ReactNode } from 'react';
import {
  Boxes,
  CircleDot,
  Download,
  FileUp,
  FlaskConical,
  Hammer,
  Landmark,
  Lock,
  LockOpen,
  Plus,
  Scale,
  Settings2,
  Shield,
  Swords,
  Users,
} from 'lucide-react';
import { MenuButton } from './components';
import { date, serverNames, silver } from './format';
import type { Session, View } from './types';

export type ViewId =
  | 'loot'
  | 'players'
  | 'ledger'
  | 'settings'
  | 'crafting'
  | 'finance'
  | 'compositions';
export const viewTitles: Record<ViewId, string> = {
  loot: 'Loot',
  players: 'Por jogador',
  ledger: 'Acertos da sessão',
  settings: 'Configurações',
  crafting: 'Crafting',
  finance: 'Financeiro consolidado',
  compositions: 'Composições',
};

function NavItem({
  id,
  current,
  icon,
  count,
  go,
}: {
  id: ViewId;
  current: ViewId;
  icon: ReactNode;
  count?: ReactNode;
  go: (view: ViewId) => void;
}) {
  return (
    <li>
      <button
        className="nav-item"
        aria-current={current === id ? 'page' : undefined}
        onClick={() => go(id)}
      >
        {icon}
        {viewTitles[id]}
        {count !== undefined && (
          <span className="count">
            <span className="visually-hidden">, </span>
            {count}
          </span>
        )}
      </button>
    </li>
  );
}

export function Sidebar({
  view,
  go,
  data,
}: {
  view: ViewId;
  go: (view: ViewId) => void;
  data: View | null;
}) {
  const totals = data?.full_totals;
  return (
    <aside className="sidebar">
      <div className="brand">
        <Swords size={22} aria-hidden />
        Kalbion
      </div>
      <nav aria-label="Navegação principal">
        <div className="nav-group">
          <h2 id="nav-session">Sessão atual</h2>
          <ul aria-labelledby="nav-session">
            <NavItem
              id="loot"
              current={view}
              go={go}
              icon={<Boxes size={18} aria-hidden />}
              count={totals?.session.events}
            />
            <NavItem
              id="players"
              current={view}
              go={go}
              icon={<Users size={18} aria-hidden />}
              count={totals ? Object.keys(totals.players).length : undefined}
            />
            <NavItem
              id="ledger"
              current={view}
              go={go}
              icon={<Scale size={18} aria-hidden />}
              count={data?.ledger.length}
            />
          </ul>
        </div>
        <div className="nav-group future">
          <h2 id="nav-future">
            Em breve <small>não implementado</small>
          </h2>
          <ul aria-labelledby="nav-future">
            {(
              [
                ['crafting', Hammer],
                ['finance', Landmark],
                ['compositions', Shield],
              ] as const
            ).map(([id, Icon]) => (
              <NavItem
                key={id}
                id={id}
                current={view}
                go={go}
                icon={<Icon size={18} aria-hidden />}
                count={<Lock size={12} aria-label="Não implementado" />}
              />
            ))}
          </ul>
        </div>
      </nav>
      <div className="sidebar-footer">
        <ul>
          <NavItem
            id="settings"
            current={view}
            go={go}
            icon={<Settings2 size={18} aria-hidden />}
          />
        </ul>
        <p className="dev-mode">
          <strong>
            <FlaskConical size={14} aria-hidden />
            Modo desenvolvimento
          </strong>
          Sem captura do jogo. Dados simulados, manuais ou importados, sempre
          identificados na tabela.
        </p>
        <span className="version">Kalbion 0.1.0</span>
      </div>
    </aside>
  );
}

export function StateBadge({ closed }: { closed: boolean }) {
  return closed ? (
    <span className="state-badge closed">
      <Lock size={12} aria-hidden />
      Encerrada
    </span>
  ) : (
    <span className="state-badge open">
      <CircleDot size={12} aria-hidden />
      Aberta
    </span>
  );
}

export function SessionHeader({
  sessions,
  session,
  busy,
  select,
  create,
  toggleClosed,
  exportAs,
  openImport,
}: {
  sessions: Session[];
  session: Session | undefined;
  busy: boolean;
  select: (id: string) => void;
  create: () => void;
  toggleClosed: () => void;
  exportAs: (format: 'csv' | 'json') => void;
  openImport: () => void;
}) {
  const closed = !!session?.closed_at;
  return (
    <header className="session-header">
      <div className="session-identity">
        <div className="session-picker">
          <label htmlFor="session-select">Sessão</label>
          <select
            id="session-select"
            value={session?.id ?? ''}
            disabled={!sessions.length || busy}
            onChange={(event) => select(event.target.value)}
          >
            {!session && (
              <option value="" disabled>
                Nenhuma sessão
              </option>
            )}
            {sessions.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
                {item.closed_at ? ' (encerrada)' : ''}
              </option>
            ))}
          </select>
        </div>
        {session && (
          <>
            <StateBadge closed={closed} />
            <dl className="session-context">
              <div>
                <dt>Servidor</dt>
                <dd>{serverNames[session.server]}</dd>
              </div>
              <div>
                <dt>Mercado</dt>
                <dd>{session.city}</dd>
              </div>
              <div className="started">
                <dt>Início</dt>
                <dd>{date(session.created_at)}</dd>
              </div>
            </dl>
          </>
        )}
      </div>
      <div className="session-actions">
        <button disabled={busy} onClick={create}>
          <Plus size={16} aria-hidden />
          Nova sessão
        </button>
        <MenuButton
          label="Ações da sessão"
          disabled={!session || busy}
          entries={[
            {
              label: closed ? 'Reabrir sessão' : 'Encerrar sessão',
              icon: closed ? (
                <LockOpen size={16} aria-hidden />
              ) : (
                <Lock size={16} aria-hidden />
              ),
              onSelect: toggleClosed,
            },
            'separator',
            {
              label: 'Exportar CSV',
              icon: <Download size={16} aria-hidden />,
              onSelect: () => exportAs('csv'),
            },
            {
              label: 'Exportar JSON',
              icon: <Download size={16} aria-hidden />,
              onSelect: () => exportAs('json'),
            },
            {
              label: 'Importar JSON',
              icon: <FileUp size={16} aria-hidden />,
              onSelect: openImport,
              disabled: closed,
              hint: 'reabra a sessão',
            },
          ]}
        />
      </div>
    </header>
  );
}

export function Summary({ view }: { view: View }) {
  const { session } = view.full_totals;
  const available = view.finance.available;
  return (
    <dl className="summary" aria-label="Resumo da sessão">
      <div className="estimate">
        <dt>Loot estimado (não é caixa)</dt>
        <dd>
          {silver(session.estimated_silver)} s
          {session.unpriced_events > 0 && (
            <small>{session.unpriced_events} sem preço</small>
          )}
        </dd>
      </div>
      <div>
        <dt>Itens válidos</dt>
        <dd>
          {silver(session.quantity)}
          <small>{session.events} registros</small>
        </dd>
      </div>
      <div>
        <dt>Jogadores</dt>
        <dd>{Object.keys(view.full_totals.players).length}</dd>
      </div>
      <div className={available < 0 ? 'negative' : ''}>
        <dt>Saldo disponível</dt>
        <dd>
          {silver(available)} s{available < 0 && <small>negativo</small>}
        </dd>
      </div>
    </dl>
  );
}

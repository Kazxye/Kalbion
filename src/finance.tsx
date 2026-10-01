import { useState, type FormEvent } from 'react';
import { Divide, Plus, Scale, Undo2 } from 'lucide-react';
import { EmptyState, Field, InlineError } from './components';
import { date, ledgerKinds, silver } from './format';
import type { Finance, Ledger, Share } from './types';

/** Session-level settlements. The future consolidated Financeiro module is a separate page. */
export function LedgerView({
  ledger,
  finance,
  busy,
  add,
  split,
  reverse,
}: {
  ledger: Ledger[];
  finance: Finance;
  busy: boolean;
  add: () => void;
  split: () => void;
  reverse: (entry: Ledger) => void;
}) {
  return (
    <>
      <p className="page-intro">
        Silver efetivamente recebido e pago nesta sessão. Loot estimado não
        entra no saldo. Erros são corrigidos por estorno, que fica no histórico.
      </p>
      <div className="ledger-head">
        <dl aria-label="Saldo da sessão">
          <div>
            <dt>Receitas recebidas</dt>
            <dd>{silver(finance.income)} s</dd>
          </div>
          <div>
            <dt>Despesas e regear</dt>
            <dd>{silver(finance.expenses)} s</dd>
          </div>
          <div>
            <dt>Acertos pagos</dt>
            <dd>{silver(finance.settlements)} s</dd>
          </div>
          <div>
            <dt>Saldo disponível</dt>
            <dd>{silver(finance.available)} s</dd>
          </div>
        </dl>
        <div className="toolbar-actions">
          <button disabled={busy} onClick={add}>
            <Plus size={16} aria-hidden />
            Novo lançamento
          </button>
          <button
            className="primary"
            disabled={busy || finance.available <= 0}
            title={
              finance.available <= 0
                ? 'Sem saldo disponível para dividir'
                : undefined
            }
            onClick={split}
          >
            <Divide size={16} aria-hidden />
            Dividir saldo
          </button>
        </div>
      </div>
      <div className="table-scroll">
        <table>
          <caption className="visually-hidden">Lançamentos da sessão</caption>
          <thead>
            <tr>
              <th scope="col">Horário</th>
              <th scope="col">Tipo</th>
              <th scope="col">Jogador</th>
              <th scope="col">Descrição</th>
              <th scope="col" className="numeric">
                Silver
              </th>
              <th scope="col">
                <span className="visually-hidden">Ações</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {ledger.map((entry) => (
              <tr key={entry.id} className={entry.reversed_by ? 'voided' : ''}>
                <td>{date(entry.occurred_at)}</td>
                <td>
                  {ledgerKinds[entry.kind]}
                  {entry.reverses && (
                    <span className="tag reversal">
                      <Undo2 size={12} aria-hidden />
                      Estorno
                    </span>
                  )}
                  {entry.reversed_by && (
                    <span className="tag reversed">
                      <Undo2 size={12} aria-hidden />
                      Estornado
                    </span>
                  )}
                </td>
                <td>{entry.player}</td>
                <td>{entry.description}</td>
                <td className="numeric amount">
                  {entry.reverses ? '−' : ''}
                  {silver(entry.amount)}
                </td>
                <td>
                  {!entry.reverses && !entry.reversed_by && (
                    <button
                      className="ghost small"
                      disabled={busy}
                      aria-label={`Estornar ${entry.description}`}
                      onClick={() => reverse(entry)}
                    >
                      <Undo2 size={14} aria-hidden />
                      Estornar
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!ledger.length && (
          <EmptyState
            icon={<Scale size={28} aria-hidden />}
            title="Nenhum lançamento nesta sessão"
            action={
              <button disabled={busy} onClick={add}>
                <Plus size={16} aria-hidden />
                Novo lançamento
              </button>
            }
          >
            Registre vendas recebidas, despesas e regear para calcular o saldo e
            dividir entre os participantes.
          </EmptyState>
        )}
      </div>
    </>
  );
}

export function LedgerForm({
  busy,
  submit,
}: {
  busy: boolean;
  submit: (data: {
    kind: string;
    player: string;
    description: string;
    amount: number;
  }) => void;
}) {
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        const data = new FormData(event.currentTarget);
        submit({
          kind: String(data.get('kind')),
          player: String(data.get('player')),
          description: String(data.get('description')),
          amount: Number(data.get('amount')),
        });
      }}
    >
      <div className="form-grid">
        <Field label="Tipo">
          <select name="kind">
            {Object.entries(ledgerKinds).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Silver (valor efetivo)">
          <input
            name="amount"
            type="number"
            min="1"
            max="1000000000000"
            step="1"
            required
          />
        </Field>
      </div>
      <Field label="Jogador ou responsável">
        <input name="player" required maxLength={64} />
      </Field>
      <Field label="Descrição">
        <input name="description" required maxLength={200} />
      </Field>
      <p className="help">
        Acerto pago reduz o saldo da sessão. Registre apenas valores realmente
        recebidos ou pagos.
      </p>
      <div className="form-actions">
        <button className="primary" disabled={busy}>
          Registrar lançamento
        </button>
      </div>
    </form>
  );
}

export function SplitForm({
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
      <Field label={`Silver a dividir (disponível: ${silver(available)} s)`}>
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
        Divisão em silver inteiro; o resto vai um silver por vez, em ordem
        alfabética. Confirmar registra um acerto pago para cada participante.
      </p>
      <InlineError message={error} />
      {shares.length > 0 && (
        <div aria-label="Prévia da divisão">
          {shares.map((share) => (
            <div className="share" key={share.player}>
              <span>{share.player}</span>
              <strong>{silver(share.silver)} s</strong>
            </div>
          ))}
        </div>
      )}
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

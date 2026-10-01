import { useState, type FormEvent } from 'react';
import { ArrowUpRight, CircleDollarSign, Plus } from 'lucide-react';
import { Field } from './components';
import { date, ledgerKinds, silver } from './format';
import type { Finance, Ledger, Share } from './types';

export function LedgerPanel({
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
      <div className="ledger-summary">
        <div>
          <span>Receitas recebidas</span>
          <strong>{silver(finance.income)} s</strong>
        </div>
        <div>
          <span>Despesas e regear</span>
          <strong>{silver(finance.expenses)} s</strong>
        </div>
        <div>
          <span>Acertos pagos</span>
          <strong>{silver(finance.settlements)} s</strong>
        </div>
        <button disabled={busy} onClick={add}>
          <Plus size={15} />
          Lançamento
        </button>
        <button
          className="primary"
          disabled={busy || finance.available <= 0}
          onClick={split}
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
              <th />
            </tr>
          </thead>
          <tbody>
            {ledger.map((entry) => (
              <tr key={entry.id} className={entry.reversed_by ? 'voided' : ''}>
                <td>{date(entry.occurred_at)}</td>
                <td>
                  <span className="pill">{ledgerKinds[entry.kind]}</span>
                  {entry.reverses && <span className="pill">Estorno</span>}
                  {entry.reversed_by && <span className="pill">Estornado</span>}
                </td>
                <td>{entry.player}</td>
                <td>{entry.description}</td>
                <td className="numeric">
                  {entry.reverses ? '−' : ''}
                  {silver(entry.amount)}
                </td>
                <td>
                  {!entry.reverses && !entry.reversed_by && (
                    <button
                      className="row-action"
                      disabled={busy}
                      onClick={() => reverse(entry)}
                    >
                      Estornar
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!ledger.length && (
          <div className="empty">
            <CircleDollarSign size={30} />
            <h3>Nenhum lançamento financeiro</h3>
            <p>
              Registre vendas recebidas, despesas e regear. Loot estimado não
              entra no saldo.
            </p>
          </div>
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
      <Field label="Tipo">
        <select name="kind">
          {Object.entries(ledgerKinds).map(([key, label]) => (
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
        Acerto pago reduz o saldo do caixa da sessão. Registre apenas valores
        efetivamente recebidos ou pagos. Erros são corrigidos por estorno.
      </p>
      <button className="primary" disabled={busy}>
        Registrar lançamento
      </button>
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

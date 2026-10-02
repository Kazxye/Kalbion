import { useState } from 'react';
import { Field } from './components';
import type { CaptureSummary } from './types';

/** Offline import of a capture file the user recorded; the file is picked natively. */
export function CaptureForm({
  players,
  busy,
  submit,
}: {
  players: string[];
  busy: boolean;
  submit: (roster: string[]) => void;
}) {
  const [roster, setRoster] = useState(players.join('\n'));
  const names = roster
    .split(/[\n,;]/)
    .map((name) => name.trim())
    .filter(Boolean);
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        submit(names);
      }}
    >
      <p className="help">
        Lê um arquivo PCAP ou PCAPNG que você gravou do seu próprio jogo. O
        Kalbion não captura tráfego, não instala drivers e não pede permissões.
        Requer o catálogo items.json da mesma versão do jogo.
      </p>
      <Field label="Jogadores da party ou guilda (um por linha)">
        <textarea
          name="roster"
          rows={6}
          value={roster}
          onChange={(event) => setRoster(event.target.value)}
          required
        />
      </Field>
      <p className="help">
        Só o loot desses jogadores é gravado; o de outros jogadores é apenas
        contado, sem nomes. A qualidade fica desconhecida, porque o evento de
        loot não a informa. Importar o mesmo arquivo de novo não duplica nada.
      </p>
      <p className="help warning-note">
        Experimental: o decoder foi testado só com capturas sintéticas e segue
        códigos de evento observados por terceiros entre 29/06 e 28/09/2026.
        Patches posteriores não foram verificados. Não há autorização da Sandbox
        Interactive para este uso.
      </p>
      <div className="form-actions">
        <button className="primary" disabled={busy || !names.length}>
          Escolher arquivo e importar
        </button>
      </div>
    </form>
  );
}

const count = (value: number, one: string, many: string) =>
  `${value} ${value === 1 ? one : many}`;

export function captureSummary(result: CaptureSummary) {
  const { report, diagnostics } = result;
  const unknownItems = Object.values(report.unknown_items).reduce(
    (sum, value) => sum + value,
    0,
  );
  const parts = [
    `Captura ${result.file_label}: ${count(result.inserted, 'inserido', 'inseridos')}, ${count(result.duplicates, 'duplicado', 'duplicados')}`,
  ];
  if (report.outside_roster)
    parts.push(
      `${count(report.outside_roster, 'loot', 'loots')} de jogadores fora da lista (não gravados)`,
    );
  if (unknownItems)
    parts.push(`${count(unknownItems, 'item', 'itens')} fora do catálogo`);
  if (diagnostics.loot_silver)
    parts.push(`${diagnostics.loot_silver} de silver ignorados`);
  if (diagnostics.loot_malformed)
    parts.push(
      `${count(diagnostics.loot_malformed, 'evento de loot ilegível', 'eventos de loot ilegíveis')}`,
    );
  if (diagnostics.file_truncated) parts.push('arquivo cortado no fim');
  parts.push(diagnostics.decoder_version);
  let text = `${parts.join('; ')}.`;
  if (result.newer_than_codebook)
    text += ` Atenção: captura posterior a ${diagnostics.codebook_observed_until}; compatibilidade com esse patch não verificada.`;
  return text;
}

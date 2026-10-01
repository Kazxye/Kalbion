import { BookOpen, KeyRound, Lock, MapPin, PlugZap, Scale } from 'lucide-react';
import { Field, InlineError } from './components';
import { cities, date, serverNames, silver } from './format';
import type { ViewId } from './shell';
import type { Bootstrap, License, Settings } from './types';

const licenseLabels: Record<License['state'], string> = {
  disabled: 'Desabilitado nesta versão de desenvolvimento',
  unauthenticated: 'Não autenticado',
  valid: 'Válida',
  expired: 'Expirada',
  unavailable: 'Serviço indisponível',
};

export function SettingsPage({
  boot,
  busy,
  save,
  importCatalog,
}: {
  boot: Bootstrap;
  busy: boolean;
  save: (settings: Settings) => void;
  importCatalog: () => void;
}) {
  const { settings, catalog, license } = boot;
  return (
    <div className="settings">
      <section aria-labelledby="settings-market">
        <h2 id="settings-market">
          <MapPin size={18} aria-hidden />
          Mercado padrão
        </h2>
        <p>
          Servidor e cidade usados como contexto de preço em novas sessões. As
          sessões existentes mantêm o contexto com que foram criadas.
        </p>
        <form
          key={`${settings.server}:${settings.city}`}
          className="row"
          onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            save({
              server: String(data.get('server')),
              city: String(data.get('city')),
            });
          }}
        >
          <Field label="Servidor">
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
              {cities.map((city) => (
                <option key={city}>{city}</option>
              ))}
            </select>
          </Field>
          <button className="primary" disabled={busy}>
            Salvar preferências
          </button>
        </form>
      </section>
      <section aria-labelledby="settings-catalog">
        <h2 id="settings-catalog">
          <BookOpen size={18} aria-hidden />
          Catálogo de itens
        </h2>
        <dl className="facts">
          <dt>Fonte</dt>
          <dd>
            {catalog.kind === 'builtin'
              ? 'Demonstrativo embutido'
              : `Importado de ${catalog.label}`}
          </dd>
          <dt>Itens</dt>
          <dd>{silver(catalog.item_count)}</dd>
          {catalog.imported_at && (
            <>
              <dt>Importado em</dt>
              <dd>{date(catalog.imported_at)}</dd>
            </>
          )}
          {catalog.skipped_count > 0 && (
            <>
              <dt>Ignorados</dt>
              <dd>{silver(catalog.skipped_count)} entradas inválidas</dd>
            </>
          )}
        </dl>
        <p>
          O Kalbion não distribui dados do jogo. Baixe{' '}
          <code>formatted/items.json</code> do repositório comunitário
          ao-data/ao-bin-dumps e importe aqui. A importação substitui o catálogo
          atual; loot já registrado mantém os nomes com que foi salvo.
        </p>
        <div>
          <button className="primary" disabled={busy} onClick={importCatalog}>
            Importar items.json
          </button>
        </div>
      </section>
      <section aria-labelledby="settings-license">
        <h2 id="settings-license">
          <KeyRound size={18} aria-hidden />
          Licença
        </h2>
        <dl className="facts">
          <dt>Estado</dt>
          <dd>{licenseLabels[license.state]}</dd>
        </dl>
        <p>
          {license.reason ?? 'Nenhuma licença validada.'} Não há validação
          offline. Histórico e exportações continuam acessíveis mesmo após a
          expiração de uma licença futura.
        </p>
      </section>
      <section aria-labelledby="settings-integrations">
        <h2 id="settings-integrations">
          <PlugZap size={18} aria-hidden />
          Integrações e diagnóstico
        </h2>
        <dl className="facts">
          <dt>Preços</dt>
          <dd>
            Manuais ou do Albion Data Project (botão Atualizar preços no Loot):
            menor oferta de venda na cidade da sessão, com a idade do dado. O
            preço manual sempre prevalece; itens de qualidade desconhecida só
            aceitam preço manual. Dados comunitários podem estar atrasados ou
            incompletos.
          </dd>
          <dt>Ícones</dt>
          <dd>
            Serviço oficial render.albiononline.com pelo core Rust, com cache
            local; itens já vistos funcionam sem conexão.
          </dd>
          <dt>Captura e OCR</dt>
          <dd>Ausentes. Nenhum privilégio de administrador é necessário.</dd>
          <dt>Log</dt>
          <dd>
            {boot.log_path ? <code>{boot.log_path}</code> : 'indisponível'}
          </dd>
        </dl>
        {boot.log_failed && (
          <InlineError message="A gravação do log falhou nesta execução (disco cheio ou sem permissão?). O aplicativo continua funcionando, mas novos eventos podem não estar registrados." />
        )}
      </section>
    </div>
  );
}

const futures: Partial<Record<ViewId, { plans: string[]; note?: string }>> = {
  crafting: {
    plans: [
      'Receitas, materiais e quantidade produzida.',
      'Custos de materiais, taxas e transporte.',
      'Retorno de recursos, uso de focus, receita líquida e margem.',
      'Preços manuais como alternativa à consulta de mercado.',
    ],
  },
  finance: {
    plans: [
      'Receitas, despesas e saldo somados entre sessões.',
      'Histórico por período e por jogador.',
      'Custos de regear e divisão acumulados.',
    ],
    note: 'Os acertos de cada sessão já funcionam em Acertos da sessão.',
  },
  compositions: {
    plans: [
      'Presets de clap, press, brawl e gank.',
      'Slots, funções, builds, jogadores e substitutos.',
      'Custo estimado de regear, com cadastro manual.',
    ],
  },
};
export function FuturePage({
  view,
  go,
}: {
  view: ViewId;
  go: (view: ViewId) => void;
}) {
  const page = futures[view];
  if (!page) return null;
  return (
    <div className="future-page">
      <span className="not-ready">
        <Lock size={12} aria-hidden />
        Não implementado
      </span>
      <p className="page-intro">
        Este módulo ainda não existe. Nada nesta página calcula ou salva dados.
        Planejado:
      </p>
      <ul>
        {page.plans.map((plan) => (
          <li key={plan}>{plan}</li>
        ))}
      </ul>
      {page.note && (
        <div>
          <p className="page-intro">{page.note}</p>
          <button onClick={() => go('ledger')}>
            <Scale size={16} aria-hidden />
            Ir para Acertos da sessão
          </button>
        </div>
      )}
    </div>
  );
}

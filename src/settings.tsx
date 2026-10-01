import { BookOpen, FlaskConical, Shield } from 'lucide-react';
import { Field } from './components';
import { cities, date, serverNames, silver } from './format';
import type { Bootstrap, License, Settings } from './types';

const licenseLabels: Record<License['state'], string> = {
  disabled: 'Desabilitado · desenvolvimento',
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
    <>
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
                {cities.map((city) => (
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
      <section className="settings-card">
        <BookOpen />
        <h2>Catálogo de itens</h2>
        <span className="pill">
          {catalog.kind === 'builtin' ? 'Demonstrativo' : 'Importado'} ·{' '}
          {silver(catalog.item_count)} itens
        </span>
        <p>
          {catalog.label}
          {catalog.imported_at &&
            ` · importado em ${date(catalog.imported_at)}`}
          {catalog.skipped_count > 0 &&
            ` · ${silver(catalog.skipped_count)} entradas inválidas ignoradas`}
        </p>
        <p>
          O Kalbion não distribui dados do jogo. Para o catálogo completo, baixe{' '}
          <code>formatted/items.json</code> do repositório comunitário
          ao-data/ao-bin-dumps e importe aqui. A importação substitui o catálogo
          atual; loot já registrado mantém os nomes com que foi salvo.
        </p>
        <div className="form-actions">
          <button className="primary" disabled={busy} onClick={importCatalog}>
            Importar items.json
          </button>
        </div>
      </section>
      <section className="settings-card">
        <Shield />
        <h2>Licenciamento</h2>
        <span className="pill">{licenseLabels[license.state]}</span>
        <p>{license.reason ?? 'Nenhuma licença validada.'}</p>
        <p>
          Esta versão funciona localmente sem autenticação. Não há validação
          offline de licença. Histórico e exportações permanecerão acessíveis
          após expiração na integração futura.
        </p>
      </section>
      <section className="settings-card">
        <FlaskConical />
        <h2>Integrações e diagnóstico</h2>
        <p>
          Albion Data Project: contrato preparado; consulta ainda não
          implementada. Preços manuais disponíveis.
        </p>
        <p>
          Ícones: serviço oficial render.albiononline.com, acessado pelo core
          Rust e guardado em cache local; sem conexão, itens já vistos continuam
          com ícone.
        </p>
        <p>
          Captura de rede e OCR: ausentes. Nenhum privilégio administrativo é
          necessário.
        </p>
        <p>
          Log do aplicativo:{' '}
          {boot.log_path ? <code>{boot.log_path}</code> : 'indisponível'}
        </p>
        {boot.log_failed && (
          <p role="alert" className="error">
            A gravação do log falhou nesta execução (disco cheio ou sem
            permissão?). O aplicativo continua funcionando, mas novos eventos
            podem não estar registrados.
          </p>
        )}
      </section>
    </>
  );
}

# Kalbion

Companion desktop local para Albion Online: Tauri 2 + Rust + React/TypeScript + Vite + Tailwind + SQLite. Interface em português, tema escuro, sem servidor de aplicação nem navegador externo. A versão de desenvolvimento usa somente dados simulados, manuais ou importados; não há captura do jogo.

## Executar

Pré-requisitos: Node.js 22+, npm, Rust estável e as dependências nativas do [Tauri 2](https://v2.tauri.app/start/prerequisites/). O app roda como usuário normal.

```bash
npm ci
npm run tauri dev
```

`npm run dev` inicia apenas o Vite; no navegador a interface avisa que o core Rust não está conectado.

### Linux

Fedora (instalar exige administrador; executar o Kalbion não):

```bash
sudo dnf install gcc gcc-c++ make openssl-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel patchelf
npm run tauri build -- --no-bundle            # executável: target/release/kalbion
npm run tauri build -- --bundles deb,rpm,appimage
```

Em Linux o app define `WEBKIT_DISABLE_DMABUF_RENDERER=1` quando a variável não existe, porque o renderizador DMA-BUF do WebKitGTK aborta em alguns ambientes Wayland com NVIDIA (`Error 71 (Protocol error)`). Para reativá-lo, exporte a variável com `0`.

### Windows

Node.js 22+, Rust com toolchain MSVC, Visual Studio Build Tools (**Desktop development with C++** e Windows SDK) e WebView2 Runtime. No PowerShell comum:

```powershell
npm ci
npm run tauri dev
npm run tauri build -- --bundles nsis   # instalador em target\release\bundle\nsis
```

Windows e instaladores **ainda não foram validados**; não há assinatura de código.

## Catálogo de itens

O Kalbion não distribui dados do jogo. Sem importação, existe só um catálogo demonstrativo de sete itens. Para o catálogo completo, baixe `formatted/items.json` de [ao-data/ao-bin-dumps](https://github.com/ao-data/ao-bin-dumps) e use **Configurações → Importar items.json**. O arquivo é escolhido pelo diálogo nativo; a importação é atômica, guarda nome do arquivo, data e contagem, e substitui o catálogo anterior. Loot já registrado mantém o nome com que foi salvo.

Os dados desses dumps derivam de arquivos do jogo, de propriedade da Sandbox Interactive. Avaliar os termos antes de redistribuir qualquer parte deles.

## Modelo de dados

- **Item:** identificado pelo `UniqueName` do Albion (`T5_BAG@1`). Tier e enchantment são derivados do ID; itens como `UNIQUE_HIDEOUT` não têm tier.
- **Quality:** pertence ao loot, não ao item. `null` significa “não informada pela fonte” e nunca é preenchida por suposição. Preços de quality desconhecida são separados das conhecidas.
- **Origem:** `simulated`, `manual` ou `observed`. Importados recebem a marca `imported`; `observed` importado é declaração do arquivo, não autenticidade verificada.
- **Deduplicação:** identidade única `(source, id)`. Replay idêntico é ignorado; mesma identidade com conteúdo diferente rejeita o lote inteiro; loots iguais com IDs diferentes são mantidos. Conteúdo e janelas de tempo nunca são usados para adivinhar identidade. Horários são normalizados para UTC antes da comparação.
- **Correções:** loot é **anulado** (continua no histórico, na exportação e na deduplicação, mas sai dos totais; pode ser restaurado). O ledger só recebe acréscimos: erros são corrigidos por **estorno**, que fica visível ao lado do original.
- **Valores:** silver inteiro (`i64`), até 10¹² por lançamento; totais limitados à faixa exata de inteiros do JavaScript. Preço ausente nunca vira zero. Estimativa de loot nunca vira receita.

### Importação JSON

```json
{
  "schema_version": 2,
  "events": [{
    "id": "source-event-0001",
    "source": "example.import.v1",
    "origin": "manual",
    "session_id": "ID-DA-SESSAO",
    "occurred_at": "2026-10-01T12:00:00Z",
    "player": "Kazz",
    "item": { "id": "T5_BAG@1", "name": "Bolsa do Especialista" },
    "quality": 2,
    "quantity": 2
  }]
}
```

`quality` pode ser `null`; `item.tier` e `item.enchantment` são opcionais e, se presentes, precisam concordar com o ID. A versão 1 (exports do primeiro build, com `quality` dentro de `item`) continua aceita. Limites: 5 MB, 10.000 eventos, sessão existente e aberta, `session_id` igual ao da sessão de destino (sem remapeamento). O arquivo de importação é um contrato próprio (`crates/kalbion-core/src/import.rs`), separado das structs de domínio. O export JSON usa o mesmo contrato em `events` e pode ser reimportado na mesma sessão; preços, anulações e ledger não são restaurados por essa via.

## Dados locais e logs

- Banco: `~/.local/share/io.kalbion.desktop/kalbion.db` (Linux) ou `%APPDATA%\io.kalbion.desktop\kalbion.db` (Windows).
- Log JSON: subpasta `logs/kalbion.log` do mesmo diretório no Linux; no Windows, o caminho aparece em **Configurações**. Rotaciona ao atingir 5 MB, inclusive durante o uso (mantém `kalbion.log.1`); falha de gravação aparece em Configurações. Sem senhas, chaves, tokens ou payloads importados.
- Migrations rodam ao abrir o banco, uma transação por versão. Antes de atualizar um banco existente, uma cópia consistente é gravada ao lado (`kalbion-v1-backup-<data>.db`). Bancos de versões futuras são recusados.
- Backup manual: fechar o app e copiar o diretório inteiro, incluindo arquivos `-wal`/`-shm` se existirem.

## Licenciamento (KeyAuth)

`LicenseProvider` fica fora do domínio. O adaptador `KeyAuth` retorna apenas `disabled` e recusa autenticação: nenhuma chamada é feita e não há sucesso simulado. Para ativar: nome da aplicação, owner ID e versão no painel KeyAuth, método de licença escolhido e política de expiração/indisponibilidade; validar o contrato atual da [Client API](https://keyauthdocs.apidog.io/getting-started/introduction), incluindo verificação de assinatura das respostas. Nunca embutir Seller API ou segredo administrativo. Tokens ficam em memória ou no armazenamento seguro do sistema, nunca em localStorage, exports ou logs.

Política: sem validação offline inventada; indisponibilidade é estado distinto de licença inválida; expiração nunca apaga histórico nem bloqueia exportação.

## Referências e limites

- [Loot-Logger-Albion-Online](https://github.com/Kazxye/Loot-Logger-Albion-Online) (projeto anterior) e [AlbionOnline-StatisticsAnalysis](https://github.com/Triky313/AlbionOnline-StatisticsAnalysis) são referências de fatos do protocolo e regras do jogo. O segundo é **GPL-3.0**: não copiar nem traduzir código dele para o Kalbion fechado.
- Os [termos da SBI](https://albiononline.com/terms_and_conditions) restringem software de terceiros e interceptação de dados. Não há autorização obtida para captura, OCR ou monetização. Captura, quando existir, será um helper isolado com privilégios mínimos; o domínio não conhece Photon.
- Antes de redistribuir Npcap, consultar a [licença OEM](https://npcap.com/oem/).

## Verificações

```bash
npm run build                                       # typecheck + Vite
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npx prettier --check src scripts
```

Teste desktop (Linux, WebKitWebDriver instalado), sempre com diretório de dados isolado:

```bash
cargo install tauri-driver --locked
npm run tauri build -- --debug --no-bundle
XDG_DATA_HOME=/tmp/kalbion-test tauri-driver --port 4446 --native-port 4447   # outro terminal
node scripts/desktop-smoke.mjs     # KALBION_SCREENSHOTS=1 grava capturas em /tmp (opcional)
```

O teste usa a interface e o IPC reais: sessão, simulação, preço, totais por jogador, ledger, divisão, filtros, importação inválida e replay, loot manual pelo catálogo, anulação, encerramento, configurações e persistência após reiniciar o processo. O diálogo nativo de arquivos (export e importação de catálogo) não é automatizado.

## Estado

- Implementado: sessões, loot com fontes explícitas, importação v1/v2, deduplicação persistente, filtros e totais, preços manuais por item e quality, anulação de loot, ledger com estorno, divisão, catálogo importável, exportação JSON/CSV, configurações, log em arquivo.
- Não implementado (páginas marcadas na interface): crafting, financeiro consolidado, composições.
- Pendente: preços do Albion Data Project (contrato `MarketPrices` existe, adaptador recusa), KeyAuth real, validação no Windows, paginação de sessões muito grandes.

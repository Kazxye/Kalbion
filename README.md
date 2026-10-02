# Kalbion

Companion desktop local para Albion Online: Tauri 2 + Rust + React/TypeScript + Vite + Tailwind + SQLite. Interface em português, tema escuro, sem servidor de aplicação nem navegador externo. A versão de desenvolvimento usa somente dados simulados, manuais ou importados (inclusive arquivos de captura gravados pelo usuário, em caráter experimental); o Kalbion não captura tráfego do jogo.

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

## Interface

Tokens de cor, tipografia, espaçamento e estados ficam no topo de `src/styles.css`. Fontes embutidas (sem download em tempo de execução): Source Sans 3 para a interface, com números tabulares, e Alegreya para o nome da sessão e títulos, ambas OFL-1.1 via `@fontsource-variable`. Tier, encantamento, qualidade, origem, anulação e estado da sessão sempre têm texto ou forma além da cor. Janela mínima 1000×700; abaixo de 1280 px de largura a coluna Jogador passa para dentro da célula do item.

## Catálogo de itens

O Kalbion não distribui dados do jogo. Sem importação, existe só um catálogo demonstrativo de sete itens. Para o catálogo completo, baixe `formatted/items.json` de [ao-data/ao-bin-dumps](https://github.com/ao-data/ao-bin-dumps) e use **Configurações → Importar items.json**. O arquivo é escolhido pelo diálogo nativo; a importação é atômica, guarda nome do arquivo, data e contagem, e substitui o catálogo anterior. Loot já registrado mantém o nome com que foi salvo.

Os dados desses dumps derivam de arquivos do jogo, de propriedade da Sandbox Interactive. Avaliar os termos antes de redistribuir qualquer parte deles.

## Ícones de itens

Os ícones vêm do serviço oficial de renderização da SBI (`render.albiononline.com`, 64 px, por item e quality) e passam pelo Rust: o webview pede `icon://localhost/<UniqueName>?quality=N` (`http://icon.localhost/...` no Windows) e nunca acessa a internet diretamente. Cache em disco no diretório de cache do app (`~/.cache/io.kalbion.desktop/icons` no Linux), válido por 30 dias e usado também offline. Itens inexistentes (404) são lembrados por 7 dias, porque o serviço demora a respondê-los; timeouts (15 s) e falhas de rede são repetidos após 2 minutos. No máximo 6 downloads simultâneos, resposta limitada a 512 KB e conferida como PNG. Sem ícone disponível, a interface mostra o ícone genérico com o tier e tenta de novo em segundo plano a cada 130 s enquanto a linha estiver visível, trocando a imagem só quando ela carrega. Falhas de conexão (como keep-alive fechado pelo servidor) são repetidas uma vez na hora.

## Preços de mercado (Albion Data Project)

**Loot → Atualizar preços** consulta o [Albion Data Project](https://www.albion-online-data.com/) (dados enviados por jogadores, API pública e somente leitura) no servidor e na cidade da sessão. O preço usado é a **menor oferta de venda** (`sell_price_min`) para o item e a quality exatos; a interface mostra a fonte e a idade do dado (`ADP, há 3 h`; acima de 24 h, marcado como antigo).

- **Manual prevalece:** a atualização nunca substitui um preço manual, nem um digitado enquanto a consulta estava em andamento. Salvar um preço sobre um preço ADP o torna manual; remover o preço deixa a próxima atualização preenchê-lo.
- **Ausência não é zero:** item sem oferta continua sem preço (um preço ADP anterior é mantido, com a idade dele). Quality desconhecida não recebe cotação, porque o mercado é por quality; só aceita preço manual. Recursos sem quality são consultados como quality 1, a convenção do ADP (conferida na API: os demais valores voltam zerados), e o preço fica guardado sem quality, como o loot.
- **Instantâneo:** como o preço manual, o preço ADP fica gravado na sessão (`recorded_at` e `observed_at`); uma sessão encerrada não muda com o mercado até alguém atualizar de novo.
- **Rede:** o core Rust fala com um host fixo por servidor (`west`, `europe` e `east.albion-online-data.com`), somente HTTPS, sem redirecionamentos, timeout de 15 s por requisição (nova tentativa imediata só quando a conexão falha em menos de 2 s), resposta limitada a 4 MB e listas de itens divididas abaixo do limite de URL; sessões com muitos itens fazem várias requisições em sequência. Linhas malformadas, com preço negativo ou data ilegível ou futura são ignoradas e contadas no log; havendo duplicatas, vale a menor oferta. Loot anulado não é consultado. Cotações (inclusive "sem oferta") ficam 5 minutos em cache de memória. HTTP 429 pausa todas as consultas pelo `Retry-After` (padrão 60 s, máximo 10 min). A consulta roda fora do lock do banco, com comando IPC próprio (`refresh_market_prices`). Falhas aparecem como "indisponível", nunca como preço.

## Captura offline (experimental)

**Ações da sessão → Importar captura (PCAP)** lê um arquivo PCAP/PCAPNG gravado pelo próprio usuário e importa o loot (evento `EvOtherGrabbedLoot`) dos jogadores de uma lista informada. Nada é capturado pelo Kalbion: sem drivers, sem privilégios, sem rede. Requer o catálogo `items.json` importado; a qualidade fica desconhecida; reimportar o mesmo arquivo não duplica. Testado só com capturas sintéticas; **não há autorização da SBI** para este uso. Fontes, campos comprovados, limites e testes: [docs/captura-offline.md](docs/captura-offline.md).

## Modelo de dados

- **Item:** identificado pelo `UniqueName` do Albion (`T5_BAG@1`). Tier e enchantment são derivados do ID; itens como `UNIQUE_HIDEOUT` não têm tier.
- **Quality:** pertence ao loot, não ao item. `null` significa “não informada pela fonte” e nunca é preenchida por suposição. Preços de quality desconhecida são separados das conhecidas.
- **Recursos sem quality:** brutos (`WOOD`, `ORE`, `HIDE`, `FIBER`, `ROCK`) e refinados (`PLANKS`, `METALBAR`, `LEATHER`, `CLOTH`, `STONEBLOCK`), inclusive encantados como `T5_HIDE_LEVEL1@1` e `T6_METALBAR_LEVEL2@2`, existem numa única quality e aparecem como **Não se aplica**. A parte do ID depois do tier precisa ser exatamente o recurso, então equipamentos como `T4_ARMOR_LEATHER_SET1` mantêm quality; conferido contra os 12.237 itens do `items.json` (248 recursos, nenhum equipamento). Qualquer quality informada para recursos (o jogo diz 1; arquivos v1 exigiam um valor) é descartada ao gravar, não rejeitada. Tier e enchantment continuam derivados do ID.
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

`quality` pode ser `null`; `item.tier`, `item.enchantment` e `item.has_quality` são opcionais e, se presentes, precisam concordar com o ID. A versão 1 (exports do primeiro build, com `quality` dentro de `item`) continua aceita. Limites: 5 MB, 10.000 eventos, sessão existente e aberta, `session_id` igual ao da sessão de destino (sem remapeamento). O arquivo de importação é um contrato próprio (`crates/kalbion-core/src/import.rs`), separado das structs de domínio. O export JSON usa o mesmo contrato em `events` e pode ser reimportado na mesma sessão; preços, anulações e ledger não são restaurados por essa via.

## Dados locais e logs

- Banco: `~/.local/share/io.kalbion.desktop/kalbion.db` (Linux) ou `%APPDATA%\io.kalbion.desktop\kalbion.db` (Windows).
- Log JSON: subpasta `logs/kalbion.log` do mesmo diretório no Linux; no Windows, o caminho aparece em **Configurações**. Rotaciona ao atingir 5 MB, inclusive durante o uso (mantém `kalbion.log.1`); falha de gravação aparece em Configurações. Sem senhas, chaves, tokens ou payloads importados.
- Migrations rodam ao abrir o banco, uma transação por versão. Antes de atualizar um banco existente, uma cópia consistente é gravada ao lado (`kalbion-v<versão anterior>-backup-<data>.db`). Bancos de versões futuras são recusados. A versão 4 move loot e preços de recursos antigos para “sem quality” (havendo preços em várias qualities, vale o mais recente), para que exports antigos continuem sendo reconhecidos como duplicados; ela roda também em bancos que já estavam na versão 3.
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
cargo test -p kalbion-core --test market -- --ignored   # opcional: ADP real; exige cotação de madeira e tábuas
KALBION_UPDATE_FIXTURES=1 cargo test -p kalbion-capture --test fixtures   # só ao mudar o construtor de fixtures
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npx prettier --check src scripts
```

Teste desktop (Linux, WebKitWebDriver instalado), sempre com diretório de dados isolado:

```bash
cargo install tauri-driver --locked
npm run tauri build -- --debug --no-bundle
mkdir -p /tmp/kalbion-dialogs
XDG_DATA_HOME=/tmp/kalbion-test XDG_CACHE_HOME=/tmp/kalbion-test-cache \
  KALBION_ADP_URL=http://127.0.0.1:4448 KALBION_TEST_DIALOG_DIR=/tmp/kalbion-dialogs \
  tauri-driver --port 4446 --native-port 4447                                         # outro terminal
KALBION_TEST_DIALOG_DIR=/tmp/kalbion-dialogs node scripts/desktop-smoke.mjs   # KALBION_SCREENSHOTS=1 grava capturas em /tmp
```

`KALBION_TEST_DIALOG_DIR` também só vale em debug: os diálogos de abrir arquivo (catálogo e captura) devolvem `items.json` e `capture.pcap` dessa pasta, que o script preenche com as fixtures sintéticas de `crates/kalbion-capture/tests/fixtures`.

`KALBION_ADP_URL` só tem efeito em builds de debug: aponta o app para o ADP simulado que o próprio script sobe na porta 4448, então o teste de preços é determinístico e não usa a rede. Capturas de tela pelo WebKitWebDriver podem exigir `GDK_BACKEND=x11` no tauri-driver.

O teste usa a interface e o IPC reais: sessão, simulação, ícones (carregados ou genéricos, nunca quebrados), preço manual, atualização de preços pelo ADP simulado (manual prevalece, quality desconhecida fica sem preço, idade exibida), importação de catálogo e de captura sintética (e sua reimportação sem duplicar), totais por jogador, ledger, divisão, filtros, importação inválida e replay, loot manual pelo catálogo, anulação, encerramento, configurações e persistência após reiniciar o processo. O diálogo nativo de arquivos (export e importação de catálogo) não é automatizado.

## Estado

- Implementado: sessões, loot com fontes explícitas, ícones oficiais com cache, importação v1/v2, deduplicação persistente, filtros e totais, preços manuais por item e quality, preços do Albion Data Project (manual prevalece), importação offline de capturas PCAP (experimental, só testada com dados sintéticos), anulação de loot, ledger com estorno, divisão, catálogo importável, exportação JSON/CSV, configurações, log em arquivo.
- Não implementado (páginas marcadas na interface): crafting, financeiro consolidado, composições.
- Pendente: KeyAuth real, validação no Windows, paginação de sessões muito grandes.

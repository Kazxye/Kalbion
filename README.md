# Kalbion

Companion desktop local para Albion Online: Tauri 2 + Rust + React/TypeScript + Vite + Tailwind + SQLite. Interface em português, tema escuro, sem Flask, servidor de aplicação ou navegador externo. A versão de desenvolvimento usa somente dados simulados, manuais ou importados.

## Executar

Pré-requisitos: Node.js 22+, npm, toolchain Rust atual com Cargo e dependências nativas do [Tauri 2](https://v2.tauri.app/start/prerequisites/). O app roda como usuário normal.

```bash
npm ci
npm run tauri dev
```

`npm run dev` inicia apenas o Vite para desenvolvimento do frontend. Abrir sua URL no navegador mostra explicitamente que o core Rust não está conectado; não existe backend falso ou persistência em localStorage.

### Linux

No Fedora, instalar os pré-requisitos de compilação (essa instalação pode exigir administrador; executar Kalbion não exige):

```bash
sudo dnf install gcc gcc-c++ make openssl-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel patchelf
```

Instalar Rust pelo método de sua distribuição ou rustup e confirmar `cargo --version`. Para Ubuntu/Debian, seguir os nomes de pacotes da documentação Tauri. Não instalar drivers de captura para esta entrega.

```bash
npm run tauri build -- --no-bundle
# Executável: src-tauri/target/release/kalbion
npm run tauri build -- --bundles deb,rpm,appimage
```

Em ambientes com erro de protocolo do Wayland, testar explicitamente X11:

```bash
GDK_BACKEND=x11 npm run tauri dev
```

### Windows

Instalar Node.js 22+, Rust com toolchain MSVC, Visual Studio Build Tools com **Desktop development with C++** e Windows SDK, além de WebView2 Runtime. Executar no PowerShell comum:

```powershell
npm ci
npm run tauri dev
npm run tauri build -- --bundles nsis
```

O instalador é gerado em `src-tauri/target/release/bundle/nsis`. Windows e instaladores não foram executados nesta entrega; exigem validação em máquina Windows antes de distribuição. Não há assinatura de código configurada.

## Fluxo inicial

1. Em Configurações, selecionar servidor e cidade. Essa preferência vale para novas sessões.
2. Criar sessão; usar **Gerar simulação** para sete eventos identificados como simulados, ou **Loot manual**.
3. Definir preços unitários; tier, enchantment e quality são separados. Preço ausente aparece como “Definir preço”, nunca zero implícito.
4. Filtrar por jogador/item/tier/enchantment/quality. Conferir os totais filtrados no rodapé e totais integrais nos cards.
5. Em **Acertos da sessão**, registrar receita efetivamente recebida, despesas e regear. Calcular divisão, revisar e confirmar pagamentos. Estimativa de loot não aumenta saldo.
6. Exportar JSON/CSV pelo seletor nativo. Exportação inclui toda a sessão e ledger, independentemente dos filtros.
7. Encerrar/reabrir sessões e reiniciar o aplicativo: histórico, preços, configurações e acertos são recuperados.

Catálogo demonstrativo com sete variantes, tiers 4–8, enchantments 0–4 e qualities 1–5. Não é um catálogo completo nem download de assets oficiais. Importação v1 está documentada em [arquitetura](docs/architecture.md); arquivos do aplicativo Python antigo não são automaticamente compatíveis.

## Dados locais

Banco `kalbion.db` no diretório `app_data_dir` do Tauri:

- Linux: normalmente `$XDG_DATA_HOME/io.kalbion.desktop/kalbion.db` ou `~/.local/share/io.kalbion.desktop/kalbion.db`.
- Windows: `%APPDATA%\io.kalbion.desktop\kalbion.db`.

Para backup, fechar o aplicativo e copiar o diretório completo. JSON/CSV são exports para consulta; o importador restaura apenas eventos na mesma sessão, não o banco completo. Erros de inicialização aparecem em diálogo. Logs JSON são emitidos em stderr; não incluem chaves, tokens ou payloads importados.

## Verificações

```bash
npm run typecheck
npm run build
npm run test:core
cargo clippy --manifest-path crates/kalbion-core/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path crates/kalbion-core/Cargo.toml -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
npm exec prettier -- --check src scripts
```

Teste desktop Linux reproduzível (WebKitWebDriver instalado):

```bash
cargo install tauri-driver --locked
npm run tauri build -- --debug --no-bundle
# Terminal separado; diretório exclusivo de teste:
GDK_BACKEND=x11 XDG_DATA_HOME=/tmp/kalbion-test tauri-driver --port 4446 --native-port 4447
# Na raiz, outro terminal:
node scripts/desktop-smoke.mjs
```

O teste cria uma sessão real no banco do processo de teste, testa a interface e o IPC, reinicia o processo e verifica recuperação. Use sempre o diretório isolado indicado. `KALBION_WEBDRIVER_URL` permite outro endereço. Screenshot em `/tmp/kalbion-desktop.png`.

Validado neste ambiente: Fedora/Linux com GTK 3.24 e WebKitGTK 2.54; frontend build/typecheck; seis testes Rust; Clippy com warnings negados no core e no shell Tauri; rustfmt e Prettier; build nativo debug e release sem bundle; fluxo desktop real via WebDriver/X11, incluindo reinício. Executável de produção: `src-tauri/target/release/kalbion`. Wayland apresentou erro de protocolo no ambiente de teste; não validado. Diálogo nativo de salvar arquivo não foi automatizado; conteúdo JSON/CSV foi testado no core.

## Entrega e próximos passos

- Implementados: sessões, loot, fontes explícitas, importação validada, deduplicação persistente, filtros/totais, preços manuais contextualizados, ledger/acertos/divisão, configurações e exports.
- Futuro: crafting, financeiro consolidado e composições são páginas explicitamente não implementadas. Acertos financeiros por sessão já funcionam.
- ADP: contrato e adaptador desabilitado; consulta HTTP/cache ainda pendentes. Próxima etapa concreta: implementar preços com hosts fixos, timeout, cache e testes de indisponibilidade.
- KeyAuth: contrato e estados explícitos, autenticação real desabilitada. Dados necessários e política offline em [arquitetura](docs/architecture.md#licenciamento).
- Captura, OCR, alertas táticos, memória e automação estão ausentes. Não foi obtida autorização SBI para integração com o jogo ou monetização. Análise do projeto de referência e fontes em [reference-analysis.md](docs/reference-analysis.md).
- Limites: sessões carregadas em memória, sem paginação, sem edição/reversão de lançamentos e sem restauração completa via JSON. Use uma instância do app por vez. Nenhum release, push ou publicação foi realizado.

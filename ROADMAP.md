# Kalbion — próximos passos

Estado em 2026-10-01: este arquivo entrou na `main` junto com as branches
`refactor/foundation` → `feature/item-icons` → `feature/ui-refresh` → `feature/adp-prices`
(fundação, ícones, interface e preços do ADP), depois da revisão independente e dos testes
manuais abaixo.

## 1. Fechar o que já está pronto — concluído

- [x] Testes manuais (com dados isolados, Wayland nativo):
  - [x] Configurações → **Importar items.json** (`formatted/items.json` do ao-data/ao-bin-dumps) → "12.237 itens".
  - [x] Ações da sessão → **Exportar JSON**, depois **Importar JSON** com o mesmo arquivo na mesma sessão → "0 inseridos; N duplicados".
  - [x] **Exportar CSV** e abrir em uma planilha.
  - [x] Navegar com **Tab** e conferir o contorno de foco.
  - [x] **Cancelar** os diálogos de arquivo → mensagem "cancelada", sem erro.
  - [x] Loot → **Atualizar preços** com rede real, incluindo recurso bruto e refinado sem preço manual.
- [x] Revisão independente (commit `e18db8c`): sem bug bloqueante; migração de cópias de bancos v2 e v3 para v4 conferida.

## 2. Preços do Albion Data Project — concluído

- [x] Contrato assíncrono (comando `refresh_market_prices`, rede fora do lock do banco), hosts fixos por região, timeout, cache de 5 min, HTTP 429 com `Retry-After`.
- [x] Fonte e idade do dado (`observed_at`) na tabela; ausência de preço continua diferente de zero.
- [x] Decisão: **preço manual prevalece**; ADP só preenche item/quality sem preço manual.
- [x] Recursos brutos e refinados (inclusive encantados) mostram "Não se aplica" e são consultados no ADP como quality 1; conferido contra os 12.237 itens do items.json e a API real.
- [x] Migration v4 separada: corrige também bancos que já estavam em v3 (teste com banco v3 sintético e bancos reais gerados pelos commits 5e7c1c3 e 1a01ed0).
- [x] Verificado por automação com rede real: cotações aplicadas a madeira, pelego, tábuas e couro .1; diálogo de preço; formulário manual; ordem do Tab; conteúdo de CSV/JSON e reimportação. Diálogos nativos, planilha e foco visível: testes manuais da seção 1.
- [ ] "Tentar novamente" no aviso de erro só recarrega a tela; não repete a atualização de preços (repetir ações genéricas seria perigoso para lançamentos).
- [ ] Futuro: escolher `sell_price_min` × `buy_price_max`, ou mediana do histórico, para estimativas mais estáveis.

## 3. Limpeza técnica

- [ ] Dividir `src/loot.tsx` (694 linhas) e `src/App.tsx` (557).
- [ ] Paginação para sessões grandes; trabalho pesado fora do mutex do IPC.
- [ ] Avaliar trocar o comando único `dispatch` por comandos separados com permissões finas.
- [ ] Windows: usar `app_local_data_dir` em vez de `%APPDATA%` (Roaming) para o banco.

## 4. Módulos futuros

- [ ] **Crafting** — primeiro com preços manuais, depois ADP. Fórmulas do Triky313 só como referência de regras (projeto GPL-3.0, não copiar código).
- [ ] **Financeiro consolidado** — saldo entre sessões e saldo por jogador (quem deve quanto, incluindo regear).
- [ ] **Composições** — presets de clap, press, brawl e gank, cadastro manual.

## 5. Antes de distribuir

- [ ] Validar no Windows: build, instalador NSIS, WebView2, caminhos, log, ícones.
- [ ] Auditoria de licenças: crates Rust e npm, texto OFL das fontes e avisos de `ring`/`webpki-roots` no instalador.
- [ ] Assinatura de código; revisar o identificador `io.kalbion.desktop` (pressupõe o domínio).
- [ ] KeyAuth real: owner ID, nome do app, política de expiração e indisponibilidade; contrato assíncrono; token no cofre de senhas do sistema.
- [ ] Jurídico: consultar a SBI sobre monetização e uso de dados; conferir termos do ao-bin-dumps e do serviço de ícones.

## 6. Só com autorização

- [ ] Captura de rede: helper isolado com privilégios mínimos, Protocol18, replay offline antes do ao vivo, identidade de evento derivada do pacote para a deduplicação.

## Notas de ambiente

- Screenshots do WebKitWebDriver só funcionam com `GDK_BACKEND=x11` neste ambiente; o smoke em si roda em Wayland nativo.
- Teste desktop sempre com diretório de dados isolado (`XDG_DATA_HOME`, `XDG_CACHE_HOME`).
- Remote: `git@github.com:Kazxye/Kalbion.git`. O repositório Loot-Logger-Albion-Online é só referência.

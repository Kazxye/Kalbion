# Verificação da primeira entrega

Ambiente: Fedora Linux, Node 22.23.1, Rust 1.98.1, GTK 3.24.52, WebKitGTK 2.54.0.

## Resultados

| Verificação | Resultado |
| --- | --- |
| `npm run typecheck` | Passou |
| `npm run build` | Passou, assets Vite gerados |
| `cargo test --manifest-path crates/kalbion-core/Cargo.toml --offline` | 6 testes passaram |
| Clippy core, all-targets, `-D warnings` | Passou |
| Clippy Tauri, `-D warnings` | Passou |
| rustfmt, ambos os crates | Passou |
| Prettier, src e scripts | Passou |
| `npm run tauri build -- --no-bundle -- --offline` | Passou, executável release Linux |
| `node scripts/desktop-smoke.mjs` | Passou na janela nativa com IPC real |
| Inspeção visual | Página inicial e tabela nativa inspecionadas |

Os testes Rust cobrem replay após reabrir banco, preservação de loots legítimos repetidos, colisão com rollback, totais por jogador/filtro, qualidade na chave de preços, ausência versus zero, separação estimativa/receita, divisão sem perda de silver, encerramento, CSV injection, exports/contexto e rejeição de overflow sem corromper dados.

O teste desktop criou sessão, simulou eventos, definiu preço, consultou jogadores, registrou receita, dividiu saldo, filtrou jogador, verificou empty state, importou versão inválida e replay válido, cadastrou loot manual, encerrou/reabriu sessão, alterou cidade, confirmou KeyAuth disabled e reiniciou o processo para verificar histórico, preço, ledger e configurações.

Banco do teste em `/tmp/kalbion-e2e-data`, separado dos dados do usuário. Screenshots `/tmp/kalbion-desktop.png` e `/tmp/kalbion-desktop-table.png`.

## Limites do que foi validado

- Não houve execução em Windows nem geração/teste de instalador ou assinatura.
- Wayland retornou erro GDK de protocolo no ambiente; os testes nativos passaram com `GDK_BACKEND=x11`.
- O seletor nativo de arquivo não foi automatizado. Serialização JSON/CSV foi testada; é necessário smoke test manual de salvar/cancelar/sobrescrever nos dois sistemas.
- Não houve consulta ADP, login KeyAuth, captura ou comunicação com o jogo.
- `.git` está vazio/protegido e não é um repositório utilizável. Não houve commit, push ou publicação. `main.py`, `.venv` e `.idea` existentes foram preservados.

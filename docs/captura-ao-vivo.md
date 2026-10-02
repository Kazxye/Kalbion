# Captura ao vivo (experimental)

Estado: **experimental, não validada com o jogo real**. Testada de ponta a ponta com o app real, o ajudante real e os datagramas sintéticos da fixture enviados pela interface `lo` (`scripts/live-smoke.mjs`). Os códigos de evento e o formato do loot são os mesmos da [captura offline](captura-offline.md), com as mesmas fontes e limitações. **Não há autorização da Sandbox Interactive**; não distribuir nem cobrar antes da resposta dela.

## Desenho

```
kalbion-sniffer (privilegiado)            app Kalbion (sem privilégios)
libpcap/Npcap → BPF "udp src port 5056" → stdout em PCAP → pcap::Reader → Pipeline
                                                         → convert (lista + catálogo) → Store::ingest
                                                         → trace (JSONL de diagnóstico)
```

- **`crates/kalbion-sniffer`**: o único binário que precisa de permissão de captura. Lista interfaces (`list`) e captura (`capture <interface>`) sem modo promíscuo, com filtro BPF fixo (`udp src port 5056`) que o chamador não pode alterar. Não interpreta nenhum byte de pacote e não grava nada em disco; escreve um fluxo PCAP clássico no stdout e logs JSON no stderr. Sai quando o stdin ou o stdout fecham, então não sobrevive ao app.
- **App**: inicia o ajudante pelo caminho fixo ao lado do executável (em debug, `KALBION_SNIFFER` pode apontar outro), lê o fluxo com o mesmo leitor e o mesmo `Pipeline` da importação offline e grava cada loot assim que chega.
- **Identidade:** `source = kalbion.capture.live`, `id = live-<uuid da execução>:<pacote>:<comando>`. Protege contra duplicata dentro da execução (retransmissões, fragmentos repetidos). **Não** deduplica entre execuções diferentes, nem entre captura ao vivo e importação de PCAP da mesma partida.
- **Auditoria:** ao parar, a execução vai para `capture_imports` (interface, versão do decoder, inseridos, duplicados, diagnóstico sem nomes).

## Permissões

**Linux.** Só `cap_net_raw`, só no ajudante (conferido: sem `cap_net_admin` a captura funciona em `lo` e `any`; sem nenhuma capability o ajudante sai com o código 3 e o app mostra o comando abaixo). Listar interfaces não exige permissão.

```bash
cargo build -p kalbion-sniffer
sudo setcap cap_net_raw=eip target/debug/kalbion-sniffer   # repetir após cada recompilação
```

Recompilar o arquivo remove a capability (comportamento do kernel). Em instalação, o binário deve pertencer ao root e não ser gravável pelo usuário.

**Windows.** Npcap instalado pelo usuário (não redistribuído), sem a opção que restringe a captura a administradores. Para compilar, o crate `pcap` precisa do Npcap SDK (`wpcap.lib` no `LIB`). **Não validado no Windows.**

## Uso

1. Configurações → **Importar items.json** (mesma versão do jogo).
2. Ações da sessão → **Captura ao vivo**: escolher a interface da conexão do jogo (ou `any` no Linux), informar os jogadores e iniciar.
3. O painel acima da tabela mostra: gravados, duplicados, fora da lista, fora do catálogo, pacotes do servidor e eventos, os últimos 30 loots vistos com o que aconteceu com cada um, e avisos (sem tráfego do Albion após 10 s, só tráfego criptografado, loot com formato inesperado, pacotes descartados pelo sistema).
4. **Parar captura**.

## Log de diagnóstico

Ativado por padrão; um arquivo por execução em `<pasta de logs>/captures/live-<data>.jsonl` (Linux: `~/.local/share/io.kalbion.desktop/logs/captures/`), limite de 50 MB. Uma linha JSON por registro:

| `kind` | Conteúdo |
|---|---|
| `start` | versão do decoder, código do loot, códigos observados, interface |
| `loot` | cada `EvOtherGrabbedLoot`: horário, pacote, índice do item, quantidade, item resolvido no catálogo (ou `null`), jogador (só se estiver na lista) e `outcome`: `inserted`, `duplicate`, `outside_roster`, `unknown_item`, `invalid_player`, `error` |
| `event` | eventos de item/loot (26, 30, 31, 32, 98, 99, 100, 279, 393) com todos os parâmetros; textos aparecem só como `<texto: N caracteres>`, exceto identificadores do jogo como `T4_BAG@1` (nomes de personagem não têm `_`) |
| `undecodable` | evento que o Protocol18 não decodificou, com código (se lido) e motivo |
| `codes` | a cada 10 s, a contagem de todos os outros códigos de evento |
| `end` | diagnóstico completo da execução |

Para conferir um caso ("peguei X às 14:32 e não apareceu"): procurar `loot` perto do horário; se não houver, olhar `event` e `codes` do mesmo intervalo. Um código que só aparece quando você pega algo indica que o loot mudou de número ou chega por outro evento.

## Limites conhecidos

- Só tráfego do servidor para o cliente (porta de origem 5056). Pedidos do cliente não são lidos.
- Hipótese não verificada: o 279 pode não cobrir todo tipo de loot (por exemplo, o próprio jogador ou baús). O log existe para responder isso com tráfego real.
- Qualidade continua desconhecida.
- Cada fragmento pendente reserva até 1 MiB (64 pendentes): quem conseguir enviar UDP com porta de origem 5056 à máquina pode forçar até ~64 MiB de memória. Orçamento total de bytes pendentes ainda não implementado.
- O ajudante não descarta a capability depois de abrir a interface.
- Empacotamento (sidecar no instalador, `setcap` no pós-instalação, detecção do Npcap) não implementado.

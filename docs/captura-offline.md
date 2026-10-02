# Captura offline de loot (PCAP)

Estado: **experimental**. Lê um arquivo PCAP/PCAPNG que o próprio usuário gravou do seu jogo e importa o loot pego por jogadores de uma lista informada. O Kalbion **não captura tráfego**, não instala drivers, não pede privilégios e não envia nada pela rede.

**Autorização:** a Sandbox Interactive **não** autorizou este uso. Os Termos (§13.3) proíbem software que "intercepts or captures data"; a tolerância declarada pela equipe em fóruns não é garantia contratual (ver a pesquisa de 2026-10-01). Este recurso não deve ser apresentado como aprovado pela SBI, nem distribuído ou cobrado antes de uma resposta dela.

**Validação:** testado **só com capturas sintéticas** construídas a partir das fontes abaixo. Nenhuma captura real foi decodificada pelo Kalbion até agora. Compatibilidade com o jogo real não está confirmada.

## Fontes consultadas (2026-10-02)

Só fatos de formato foram usados; nenhum código foi copiado ou traduzido.

| Projeto | Commit | Licença | Uso |
|---|---|---|---|
| [Nouuu/Albion-Online-OpenRadar](https://github.com/Nouuu/Albion-Online-OpenRadar) | `69a76de` (2026-09-23) | MIT | `internal/photon/packet.go`, `deserializer.go`, `typecodes.go`; `docs/technical/PROTOCOL18_PARAM_LAYOUTS.md` e `PROTOCOL18_OBSERVED_CODES.md` (código real no parâmetro 252; 279 observado no corpus de 2026-09-03) |
| [ao-data/albiondata-client](https://github.com/ao-data/albiondata-client) | `12ff34e` (2026-09-16); enum de eventos em `a4290fd` (2026-08-31) | MIT | `client/photon/parser.go` (flags 1 e `0xCC`, CRC, pacotes concatenados), `client/events.go` (`evOtherGrabbedLoot` = 279) |
| [synthalorian/AlbionOnline-Translator](https://github.com/synthalorian/AlbionOnline-Translator) | `dbe9003` (2026-09-15) | Apache-2.0 | `src-tauri/src/photon.rs` (fragmentos, tipos do Protocol18) |
| [designsvet/ao-loot-logger](https://github.com/designsvet/ao-loot-logger) | `307c050` (2026-09-29) | GPL-3.0 (**só fatos**) | `src/config.js` (277 → 279 em 2026-07-07), `ev-other-grabbed-loot.js` (significado dos parâmetros 1–5) |
| [ao-data/ao-bin-dumps](https://github.com/ao-data/ao-bin-dumps) `formatted/items.txt` e `items.json` | master em 2026-10-02 | sem licença declarada (o usuário importa) | o número do item no evento é o `Index` do catálogo; os 12.237 itens têm o mesmo índice nos dois arquivos |

Dependências novas: nenhuma crate nova no lockfile. `sha2` 0.10.9 (MIT OR Apache-2.0) já vinha pelo Tauri. Leitor de PCAP/PCAPNG, IP/UDP, Photon e Protocol18 foram escritos no Kalbion.

## O que está comprovado e o que não está

Comprovado por duas ou três fontes independentes (não por tráfego real lido pelo Kalbion):

- Photon sobre UDP, servidor na porta 5056. Cabeçalho de 12 bytes; flags `0` normal, `0xCC` com CRC-32 (polinômio IEEE refletido, sem XOR final) de 4 bytes, `1` criptografado. Comandos com cabeçalho de 12 bytes: 6 confiável, 7 não confiável (+4 bytes), 8 fragmento (+20 bytes: início, quantidade, número, tamanho total, deslocamento).
- Mensagem: byte de sinal, tipo (`4` evento, `131` criptografada), depois Protocol18: byte de despacho, tabela de parâmetros (contagem compacta, chave, tipo, valor).
- Tipos do Protocol18 em que as fontes concordam: bool, byte, short, float, double, string, inteiros compactados (zigzag), Int1/Int2/Long1/Long2 e negativos, tipos "zero", custom e custom "slim" (0x80–0xE4), dicionário com tipos simples, array de objetos, arrays tipados de bool, byte, short, float, double, string, int e long compactados, e array de custom.
- O código real do evento vai no parâmetro 252. `EvOtherGrabbedLoot` = **279** desde o patch de 2026-06-29 (era 277); visto por terceiros em tráfego real até 2026-09-28.
- Parâmetros do 279: 1 = de quem (contêiner ou vítima), 2 = quem pegou, 3 = é silver, 4 = item (índice numérico), 5 = quantidade.

Desconhecido ou não confirmado:

- Se o 279 real traz outros parâmetros. O decoder exige 1, 2, 4 e 5 com o tipo certo e aceita parâmetros extras.
- A largura exata dos inteiros (o decoder aceita qualquer tipo inteiro).
- **Qualidade:** o evento não traz. Fica desconhecida; cruzar com outros eventos (30, 32, 98, 99) não foi implementado.
- Versão do jogo: o PCAP não a informa. Capturas posteriores a 2026-09-28 são aceitas se o formato bater, mas a interface avisa que o patch não foi verificado.
- Tipos em que as fontes divergem (`Hashtable` 21, array aninhado 0x40, arrays de dicionário, dicionários com tipos aninhados, códigos 229–255): o decoder **não adivinha**; o evento que os contém vira "não decodificado" no diagnóstico.

## Desenho

```
arquivo → pcap (contêiner) → net (enlace/IP/UDP) → photon (transporte) → protocol18 (valores)
        → albion (código 279) → convert (catálogo + lista de jogadores) → Store::ingest
```

- Crate `crates/kalbion-capture`, sem Tauri. O core continua sem conhecer Photon; ele só ganhou `catalog_item_by_game_index`, a tabela de auditoria `capture_imports` (migration 5) e `capture_session_elsewhere`.
- **Identidade:** `source = kalbion.capture.pcap`, `id = <impressão digital>:<pacote>:<comando>`. A impressão digital é o SHA-256 do início do arquivo até o fim do primeiro pacote: uma cópia feita durante a gravação e o arquivo final dão os mesmos IDs para os pacotes em comum. O conteúdo nunca decide identidade. Converter o arquivo para outro formato (PCAP ↔ PCAPNG) muda a impressão digital.
- **Duplicatas de transporte:** comandos confiáveis repetidos (mesmo canal e sequência) e fragmentos repetidos são descartados antes de virar evento.
- **Mesma captura em duas sessões:** recusada com o nome da sessão onde ela já está; o core também rejeita os eventos.
- **Jogadores:** só os da lista (1 a 300 nomes, sem diferenciar maiúsculas); os outros são contados, sem nome. O nome de quem foi saqueado nunca é gravado. Silver é ignorado.
- **Itens:** pelo `Index` do catálogo importado; índice desconhecido é relatado, não adivinhado. Sem catálogo importado, a importação é recusada.
- **Registro:** cada importação grava em `capture_imports` o nome do arquivo, SHA-256, impressão digital, versão do decoder, inseridos, duplicados e o diagnóstico (contagens, sem nomes), e também vai para o log.

## Limites

| O quê | Limite |
|---|---|
| Arquivo | 512 MB |
| Registro de pacote | 256 KB |
| Bloco PCAPNG que não é pacote | 16 MB (pulado) |
| Mensagem Photon remontada | 1 MB, até 4.096 fragmentos |
| Mensagens fragmentadas incompletas ao mesmo tempo | 64 (a mais antiga é descartada e contada) |
| Valores por mensagem Protocol18 | 65.536; profundidade 8; contagens limitadas aos bytes restantes |
| Parâmetros por evento | 256, sem chave repetida |
| Eventos por lote gravado | 10.000 (lotes atômicos; reimportar completa o restante) |

## Recusas explícitas

A importação falha, sem gravar nada, quando:
- o arquivo não é PCAP/PCAPNG, não tem pacotes ou só tem pacotes sem horário (Simple Packet Block);
- não há tráfego UDP vindo da porta 5056, ou só há tráfego criptografado;
- a captura é anterior a 2026-06-29 (outro código de evento);
- metade ou mais dos eventos não decodifica (≥5), o que indica outro protocolo;
- metade ou mais dos eventos 279 têm outro formato, o que indica outra versão do jogo.

## Testes

Todos **sintéticos** (`crates/kalbion-capture/tests`):

- `decode.rs`:
  - bytes escritos à mão conferem o construtor de fixtures;
  - eventos conhecidos com resultado exato (confiável, não confiável, fragmentado fora de ordem e repetido, retransmissão, CRC válido e inválido, criptografado, pacotes concatenados, silver, outros códigos);
  - PCAP little endian µs, PCAP big endian ns com IP cru, e PCAPNG com Linux SLL dão o mesmo resultado;
  - determinismo;
  - arquivo cortado em **cada byte**: nunca entra em pânico e mantém a identidade;
  - 8.000 mutações aleatórias sem pânico;
  - limites e recusas;
  - tipos disputados.
- `import.rs`: reimportar o mesmo arquivo não duplica; filtro da lista; item fora do catálogo; mesma captura em outra sessão; convivência com loot manual, importação JSON, anulação e preços manuais e do ADP; catálogo, lista e sessão encerrada.
- `fixtures.rs`: os arquivos versionados em `tests/fixtures` continuam iguais ao construtor.
- Smoke desktop: importa catálogo e captura sintética pela interface real, duas vezes (3 inseridos, depois 3 duplicados).
- Conferência independente: `tshark`/`capinfos` leem o PCAP sintético sem avisos; versões convertidas pelo `editcap` (PCAPNG e PCAP em ns) dão as mesmas observações.

**Não testado:** nenhuma captura real do Albion. Antes de usar com dados reais, validar com um arquivo gravado legitimamente pelo próprio usuário e comparar com o que aconteceu no jogo.

## Fora do escopo

Qualidade do item; radar, alertas de PK, overlay ou qualquer rastreamento de outros jogadores. A captura ao vivo usa este mesmo decoder e está em [captura-ao-vivo.md](captura-ao-vivo.md).

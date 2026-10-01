# Referência e limites de integração

Consultado em 2026-10-01. O clone HTTPS falhou; arquivos foram lidos pelo conector GitHub autenticado. Nenhum código Python foi copiado nem empacotado.

## Conhecimento aproveitado

- [items_service.py](https://github.com/Kazxye/Loot-Logger-Albion-Online/blob/main/services/items_service.py), blob `e1674b1d3aef7b360d43558bf6c0d39866dbcaf5`: mapeia Index numérico para UniqueName e LocalizedNames, fallback PT-BR/EN-US/ID, cache 24h e fallback JSON/TXT/cache antigo. Kalbion começa com catálogo pequeno e independente, sem depender de download. Uma importação futura de catálogo deve preservar versão e proveniência e não ignorar erros silenciosamente.
- [data_handler.py](https://github.com/Kazxye/Loot-Logger-Albion-Online/blob/main/handlers/data_handler.py), blob `79780b06ecced51c14842ef49c11a7749d51f4f2`: dispatcher Photon com tipo real nos parâmetros 252/253 e correlação de objetos. Esses detalhes pertencem a um possível adaptador futuro, nunca ao domínio.
- [events_config.py](https://github.com/Kazxye/Loot-Logger-Albion-Online/blob/main/services/events_config.py), blob `db87593408faf711d1d4d591c20777d0dc2ee64d`: códigos locais sujeitos à versão do jogo. Não transportar números como verdade permanente.
- [ev_other_grabbed_loot.py](https://github.com/Kazxye/Loot-Logger-Albion-Online/blob/main/handlers/events/ev_other_grabbed_loot.py), blob `e90d9425dff63a3aadfce5db0ff9024ed4e66ca0`: separa silver de itens, resolve jogador, ID numérico e quantidade. Quality não é estabelecida por esse handler; não inferir quality de enchantment.
- [op_inventory_move_item.py](https://github.com/Kazxye/Loot-Logger-Albion-Online/blob/main/handlers/requests/op_inventory_move_item.py), blob `589d0813b01598f227827a4b1e40639c64df20a1`: usa containers/slots e estado local para loot próprio. Uma request de movimento não prova confirmação do servidor; futura normalização precisa distinguir tentativa de resultado observado e lidar com perdas/reordenação.
- [loot_event.py](https://github.com/Kazxye/Loot-Logger-Albion-Online/blob/main/models/loot_event.py), blob `799e92ceb620193a5ba2469517bb0e7beef4a7f1`: modelo não inclui ID estável, sessão, origem e quality explícita. Kalbion exige esses campos, timestamps com timezone e deduplicação persistente.

## Autorização não é viabilidade técnica

Os [termos oficiais SBI, seção 13.3](https://albiononline.com/terms_and_conditions) restringem software de terceiros e interceptação/captura de dados. Não foi obtida autorização específica para Kalbion, captura passiva, OCR ou monetização. A existência de outras ferramentas e alegações no README não constituem autorização. Antes de captura ou distribuição comercial, solicitar esclarecimento à SBI sobre o escopo exato. Não transformar silêncio em permissão.

O [Albion Data Project](https://www.albion-online-data.com/) é fonte comunitária; sua existência não autoriza automaticamente outros mecanismos de coleta. Kalbion não captura, não faz radar, não inspeciona terceiros e não envia pacotes ao jogo.

Não há dependência Npcap nesta entrega. Antes de eventual redistribuição consultar [licenciamento Npcap](https://npcap.com/oem/); acesso gratuito e redistribuição são questões diferentes. Auditar licenças do catálogo/dumps e de qualquer código reutilizado antes de distribuir. Os lockfiles registram as dependências efetivamente resolvidas.

Outras fontes oficiais: [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), [capabilities](https://v2.tauri.app/security/capabilities/), [WebDriver](https://v2.tauri.app/develop/tests/webdriver/), [KeyAuth Client API](https://keyauthdocs.apidog.io/getting-started/introduction).

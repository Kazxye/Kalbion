# Arquitetura e contratos

`src` apresenta dados e envia comandos Tauri. `src-tauri` contém IPC, seleção nativa de arquivo e inicialização. `crates/kalbion-core` contém domínio, adaptadores, persistência e licenciamento independente. Não há servidor de aplicação, Python, leitura de memória, captura ou automação do jogo.

## Eventos

Contrato v1 para importação, com horário RFC 3339 e quantidade inteira positiva:

```json
{
  "schema_version": 1,
  "events": [{
    "id": "source-event-0001",
    "source": "example.import.v1",
    "origin": "manual",
    "session_id": "COPIE-O-ID-DA-SESSAO-NO-DIALOGO",
    "occurred_at": "2026-10-01T12:00:00Z",
    "player": "Kazz",
    "item": { "id": "T5_BAG@1", "name": "Bolsa do Especialista", "tier": 5, "enchantment": 1, "quality": 2 },
    "quantity": 2
  }]
}
```

Identidade única global: `(source, id)`. Reenvio com conteúdo igual é ignorado; mesma identidade com conteúdo diferente rejeita o lote inteiro. Dois loots iguais com IDs diferentes permanecem, mesmo com jogador/item/horário idênticos. Simulação e registro manual geram UUIDs a cada novo evento. Importadores devem preservar IDs entre replays: gerar UUID novo em cada leitura elimina a deduplicação. Não se deduz identidade de conteúdo nem de janelas de tempo. Não há correlação automática entre fontes diferentes.

`origin` distingue `simulated`, `manual` e `observed`. A importação conserva essa declaração e acrescenta `imported=true` na persistência: observado importado NÃO significa autenticidade verificada. A primeira versão não aceita eventos observados por captura. A interface não permite marcar registros manuais como observados. IDs de item usam tier e enchantment explícitos e consistentes; quality é independente, de 1 a 5.

Importação: até 5 MB e 10.000 eventos, transação atômica, sessão existente e aberta, sem remapeamento implícito. Pode reimportar o JSON exportado selecionando a mesma sessão; a UI extrai `schema_version` e `events`. É uma importação de eventos, NÃO restauração de backup financeiro.

## Persistência e valores

SQLite com migration transacional `user_version=1`, WAL, foreign keys, synchronous FULL, busy timeout e SQL parametrizado. Mutex serializa comandos dentro do processo. Campos externos são validados antes de escrever. Versões de banco posteriores são rejeitadas. Nenhum comando permite SQL, caminhos arbitrários, shell ou exclusão de histórico pelo frontend.

Silver inteiro (`i64`), valores individuais limitados a 10^12. Multiplicação e soma verificadas; totais restringidos à faixa inteira exata do JavaScript. Ausência de preço é `null`; zero informado é preço conhecido. Preço é por sessão + item + quality, com região, cidade, origem manual e horário. Atualizar preço reavalia todos os eventos correspondentes da sessão; não há histórico de revisões de preço nesta versão. Sessões congelam região/cidade no momento da criação.

Estimativa de loot não gera receita. Ledger append-only registra receita recebida, despesa, regear e acerto pago. Saldo = receitas − despesas − acertos. Divisão confirmada debita o caixa em transação; resto inteiro é distribuído em ordem alfabética. Despesas podem resultar em saldo negativo. Não há desfazer/correção de lançamentos nesta versão; conferir antes de confirmar. Encerrar sessão bloqueia ingestão; acertos e preços continuam disponíveis.

Exports são integrais, ignoram filtros, e incluem eventos, preços e financeiro. CSV usa tipos de linha `loot`/`ledger`, escaping padrão e neutralização de fórmulas em texto não confiável. JSON mantém os valores originais. O caminho de exportação vem exclusivamente do seletor nativo; cancelamento é distinguido de sucesso. Mantenha backup do banco com o aplicativo fechado, incluindo arquivos WAL/SHM se ainda existirem.

## Licenciamento

`LicenseProvider` é independente do domínio; `KeyAuth` retorna somente `Disabled` e recusa autenticação. Estados previstos: disabled, unauthenticated, valid, expired e unavailable. Nenhuma chamada de autenticação é feita, nenhum token é armazenado e não existe sucesso simulado.

Para ativar: nome da aplicação, owner ID, versão da aplicação, método de licença/login aprovado e política de expiração/indisponibilidade; validar o contrato Client API e verificação de resposta atuais. Nunca embutir Seller API, segredo administrativo ou chave de assinatura. Se um protocolo exigir segredo compartilhado que precise permanecer secreto, um cliente distribuído não oferece essa fronteira: revisar o desenho antes de integrar. Tokens de sessão deverão ficar em memória ou armazenamento seguro do SO, nunca localStorage, exports ou logs.

Política atual: build de desenvolvimento totalmente local e explicitamente sem validação de licença. Política planejada de produção: sem concessão offline inventada; indisponibilidade deve ser distinta de licença inválida, e histórico/exportação sempre acessíveis. Implementar e testar essa política junto com a integração real.

## Albion Data Project e captura futura

`MarketPrices`, `PriceRequest` e `Price` são o ponto de integração. Adaptador ADP retorna erro explícito de não implementado. Próxima implementação deve usar hosts fixos por região, timeout total (sugestão 8s), cache por região/cidade/item/quality (sugestão 5 min), limites de lote e resposta, backoff para 429/5xx e horário de observação separado de consulta. Cache expirado precisa ser identificado; falha e preço zero da API não podem virar preço conhecido automaticamente. Não aceitar URL arbitrária fornecida pelo frontend.

Futura captura, somente após esclarecimento de autorização, deve ser helper isolado com mínimos privilégios, IPC autenticado/local e protocolo de eventos limitado. O domínio nunca deverá conhecer Photon, códigos de evento ou interfaces de rede. Sem Npcap/libpcap distribuídos nesta entrega.

## Segurança e limites

Capability apenas da janela local `main`, comandos `dispatch` e `export_session`; sem shell, HTTP, filesystem genérico ou permissões remotas. CSP bloqueia scripts externos, frames e objetos. React escapa textos importados. O modelo protege a fronteira webview → core, não um usuário local malicioso com acesso ao processo/banco. Banco não é criptografado. Logs JSON vão para stderr, contendo operação/IDs/erros, sem payloads, senhas ou licenças; não há arquivo de log persistente gerenciado.

Esta primeira versão carrega os eventos da sessão em memória; para sessões grandes, próxima melhoria é paginação SQL. Não há suporte validado a múltiplas instâncias simultâneas realizando acertos na mesma sessão.

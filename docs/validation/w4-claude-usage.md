# W4 — widget Claude usage (leitura local)

Ambiente: Windows 11 build 26200, 150% de DPI, release com asserts de debug.

## O que existe

- **Serviço `ClaudeUsage`** (`pod-model::usage`, `claude_usage.rs`): `Effect::ClaudeUsage(bool)` assina e
  `Event::ClaudeUsage(UsageSnapshot)` entrega. É a capacidade `ReadFiles` do ADR-0001 reduzida a **uma pasta
  permitida**, `~/.claude/projects` (só leitura, nada de rede, sem chave). Thread própria, a cada 60 s, **só enquanto
  algum widget visível assina**.
- **Leitura incremental**: o primeiro ciclo lê os `.jsonl` tocados nos últimos 8 dias; os seguintes só leem o que foi
  acrescentado desde o deslocamento guardado de cada arquivo. Linha incompleta (o Claude Code ainda escrevendo) espera
  o ciclo seguinte; arquivo truncado é relido; arquivo velho sai da lista.
- **O que se extrai**: de cada linha `assistant` só `message.usage` (entrada, saída, leitura e criação de cache) e o
  `timestamp`; uma mesma resposta aparece em várias linhas e **conta uma vez** (a última substitui a anterior, por
  `message.id`, com `requestId`/`uuid` como reserva). O texto das conversas nunca é guardado nem chega ao widget.
- **Dia local**: o fuso vem de `GetTimeZoneInformation` (horário de verão incluído), então "hoje" vira à meia-noite local.
- **"Tokens"** no cartão = entrada + saída + criação de cache; a leitura de cache, muito maior, aparece à parte.
- **Widget `pod.claude-usage`** (1×1, 2×1, 2×2): 1×1 total de hoje; 2×1 total, nº de respostas e gráfico de 7 dias;
  2×2 total grande, gráfico, total de 7 dias, cache lido e respostas. Pasta ausente: "Sem dados". Categoria Sistema.

## Verificado

- 231 testes no `pod-app` e 136 no `pod-model`: datas civis e ISO, dia local por fuso, só linhas `assistant` com
  uso, deduplicação, janela de 7 dias (antigos e futuros ficam fora), formato "1,2 mi"; no provedor, leitura incremental
  (segundo ciclo lê 0 bytes; acrescento lê exatamente os bytes novos e não duplica), linha pela metade, truncamento,
  pasta inexistente, arquivos velhos, e a thread real (entrega um resumo e para); no widget, assina/cancela e esquece.
- **Ao vivo com os seus dados reais** (só números, sem conteúdo): 96 arquivos, 1,24 GB lidos no primeiro ciclo em
  **~1 s**; hoje 4,8 mi de tokens e 675 respostas; 7 dias: 32 mi, cache lido 1,6 bi. Os três cartões mostraram os
  mesmos números. Depois do aquecimento: 0,002% de CPU, 39,9 MB de memória privada, handles estáveis.

## Não verificado / limites

- **Os números são uma estimativa local**: vêm do `usage` registrado nos transcritos, não da cobrança da Anthropic;
  respostas de subagentes entram se estiverem nos arquivos, e transcritos apagados somem da conta. Não confrontei com o
  painel oficial de uso nem com o limite do plano (não há "porcentagem do limite").
- O primeiro ciclo lê tudo o que for dos últimos 8 dias (aqui 1,2 GB): em disco lento pode levar vários segundos (numa
  thread, sem travar a interface). A memória guarda um registro por resposta dos 7 dias.
- Arquivos de outra instalação ou de outro usuário do Windows não são vistos.
- Sem custo em dinheiro (preços mudam e dependem do modelo; não inventei valores).

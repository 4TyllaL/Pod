# W3 — Rede, pastas conhecidas e área de transferência

Ambiente: Windows 11 build 26200, build de debug. Escopo do ADR-0001 §5 (W3) **menos** o que já existia
(`Chart` e métricas foram entregues no W1/W2) e **menos** Activity/Memory (já são CPU/Memória).

## O que existe

- **`Chart`**: já estava no motor (view, layout, renderer, UIA com `label`). W3 só o usa pela primeira vez
  em um widget (Rede), conforme a regra "primitiva só entra com dois widgets" — o segundo é o histórico de
  CPU, ainda não feito (ver pendências).
- **Rede (`pod.network`)**, 1×1/2×1/2×2: download e upload em KB/s ou MB/s e gráfico dos últimos 2 min
  (60 leituras a cada 2 s, escala mínima de 100 KB/s para o ruído não virar montanha).
  - `MetricsNeeds.network`, `NetCounters`, `net_rate` (delta e tempo decorrido; contador que volta = sem ponto,
    nunca um pico falso), `format_rate`. Provedor: `GetIfTable2` somando interfaces **ativas**, sem loopback
    nem túneis de software; primeira leitura só marca o ponto de partida.
- **Serviço `Local`** (`pod-model::local`, `localdata.rs`): `LocalNeeds`, `Effect::Local`, `Event::Local`,
  assinaturas por instância no runtime (só instâncias visíveis e em execução) e prazo no shell (1 s com
  área de transferência, 10 s só com pastas). Sem assinante: sem prazo, sem leitura, cache descartado.
- **Downloads (`pod.downloads`) e Capturas (`pod.screenshots`)**: contagem e 5 arquivos mais novos
  (nome abreviado mantendo a extensão, tamanho e idade em pt-BR). Pastas via `SHGetKnownFolderPath`; ignora
  subpastas e arquivos ocultos; lê no máximo 5 000 entradas. O widget **nunca recebe caminhos**.
- **Área de transferência (`pod.clipboard`)**: prévia de texto (colapsa espaços, 200 caracteres). Só consulta o
  número de sequência a cada tick e relê o texto quando ele muda. Conteúdo marcado com o formato
  `ExcludeClipboardContentFromMonitorProcessing` (gerenciadores de senha) **não é lido**: aparece como
  "Conteúdo privado". Imagem/arquivos aparecem como "Imagem ou arquivos". A prévia fica só em memória e é
  descartada ao remover o widget. Nenhum widget grava esses dados em disco (`snapshot` vazio).
- Bandeja: "Novo widget" ganhou Rede, Downloads, Capturas de tela e Área de transferência.
- Galeria: 36 imagens novas (4 widgets × 3 formatos × 3 temas), com dados de demonstração fixos
  (nomes fictícios; a galeria não lê o disco nem a área de transferência).

## Correção encontrada no caminho

`Runtime::apply` removia `metrics_subs` dentro do braço `Effect::TimeTicks(None)` (linha fora de lugar), e
`Runtime::remove` não limpava as assinaturas de métricas. Sem efeito visível hoje (o filtro por instância
viva escondia o resto), mas um widget que cancelasse só a hora perderia as métricas. Movido para `remove`,
junto com a nova limpeza de `local_subs`.

## Testes (todos passam: 105 no `pod-model`, 191 no `pod-app`)

- Regras puras: `net_rate` (delta, contador que volta, intervalo zero), `format_rate`, união e intervalo de
  `LocalNeeds`, listagem (mais novos, contagem total), prévia por caracteres (não corta no meio), idade, nome
  abreviado com extensão.
- Provedor: sem pedido nenhuma leitura é contada; só o que foi pedido é lido; pasta com subpasta é contada
  corretamente; pasta inexistente = "sem dados", não "0 arquivos"; sem assinante o texto não fica guardado.
- Widgets: assinam só o que mostram e cancelam ao parar; só redesenham quando o conteúdo muda; cabem em todo
  formato com e sem dados e em todos os estados da área de transferência (vazia, privada, outros, longa).
- Regressão visual: os 204 hashes anteriores não mudaram; 36 novos gravados.

## Não verificado (honesto)

- **Nenhum teste ao vivo com janelas reais** destes quatro widgets (criar pela bandeja, ver o gráfico
  correr, copiar texto e ver a prévia). A cobertura vem de testes de regra, do provedor real chamado em teste
  e da galeria offscreen.
- Medição de custo em repouso com 10 widgets de rede/pasta/clipboard (portão do ADR) **não foi feita**.
- Área de transferência: o formato de exclusão cobre gerenciadores de senha que o respeitam; um app que não o
  marca terá o texto exibido. Histórico da área de transferência do Windows não é consultado.
- Rede: soma todas as interfaces ativas; VPN e adaptadores virtuais podem contar o tráfego em dobro.
- Cliques nos arquivos (abrir/mostrar na pasta) não existem: exigiria um efeito `OpenPath` validado pelo host.
- O segundo uso do `Chart` (histórico de CPU) e Battery com mais formatos ficaram fora.

# W2 (antecipado do W3) — widgets de métricas do sistema

Pedido do dono: começar o W2 pelos widgets de métricas (CPU, memória, disco, bateria). No ADR-0001 as
métricas estavam no W3; foram **antecipadas** (sem a primitiva `Chart` e sem Network, que não foram
pedidas). O disco entrou além da lista do ADR (`GetDiskFreeSpaceExW` no disco do sistema).

## O que existe

- `pod-model::metrics`: `MetricsNeeds`, `CpuTimes`/`cpu_usage` (delta entre duas leituras), `Battery::from_status`
  (interpreta `SYSTEM_POWER_STATUS`), `Sample`, `format_bytes` (vírgula decimal), `format_minutes`.
- Contrato: `Effect::Metrics(Option<MetricsNeeds>)` assina/cancela; `Event::Metrics(Sample)` entrega.
- Runtime: assinaturas por instância; só contam instâncias **visíveis e em execução**; `metrics_needs()` é a
  união do que elas pedem.
- Shell: prazo único de leitura (2 s se há CPU ou memória; 30 s se só disco/bateria). Sem assinante visível:
  sem prazo, sem leitura, e o ponto de partida da CPU é esquecido. Ao voltar da suspensão ou reaparecer, relê.
- `sysmetrics.rs`: provedor (`GetSystemTimes`, `GlobalMemoryStatusEx`, `GetDiskFreeSpaceExW`,
  `GetSystemPowerStatus`) com contadores de leitura por tipo, despejados no log por `WM_METRICS_DUMP`.
- Widgets `pod.cpu`, `pod.memory`, `pod.disk`, `pod.battery` (1×1, 2×1, 2×2) sobre o desenho `gauge`
  compartilhado com o Time progress (o refactor não alterou nenhum dos 54 hashes anteriores). Alerta por cor:
  aviso ≥ 80% e perigo ≥ 95% de uso; bateria ≤ 20% e ≤ 10%, positivo ao carregar. Sem leitura: anel vazio e
  "—", sem barra de progresso. O cartão só pede quadro quando o percentual inteiro ou o texto mudam.
- Bandeja: "Novo widget" lista os seis cartões.
- Galeria: 36 imagens novas (4 widgets × 3 formatos × 3 temas), 90 no total, com leitura de demonstração fixa.

## Medições (release com asserts de debug, 12 núcleos, 3 monitores, GPU real)

| Cenário | Resultado |
|---|---|
| Só relógios (nenhum assinante de métricas), 40 s | leituras: 0/0/0/0 antes e depois; 0,000% de CPU |
| 10 widgets (3 CPU, 3 memória, 2 disco, 2 bateria), 60 s | 90 quadros; 0,009% de CPU da máquina (0,063 s); p95 de UI 0,56 ms; leituras ≈ 30 por tipo em 60 s (a cada 2 s); memória privada 36,5 MB |
| Só disco e bateria, 65 s | 0 quadros; leituras de disco e bateria 1 → 3 (a cada 30 s); CPU e memória: 0 leituras; 0,000% de CPU |

UIA lido nos cartões: `99%, Disco, 2,8 GB livres`, `—, Bateria, Na tomada` (a versão medida ainda anunciava
"Progresso: 0%" para a bateria sem leitura; a barra foi removida nesse caso e o desenho conferido só na galeria).

## Testes

- `pod-model`: uso de CPU (deltas, intervalos vazios, contador que volta), união/intervalo de necessidades,
  formatação, mapeamento da bateria.
- Runtime: assinantes visíveis apenas; remover cancela; leitura repetida igual não invalida.
- Provedor: sem pedido não chama o Windows (contador local), memória e disco reais, CPU precisa de duas leituras.
- Widgets: assinam exatamente o que mostram, cancelam ao parar, textos em pt-BR, papéis de alerta, placeholders,
  todos os formatos cabem no cartão.
- Corrigidos dois testes instáveis: o do worker de storage (esperava o despertar antes de ele acontecer) e o do
  provedor (contadores globais compartilhados entre testes em paralelo).

## Limites e observações

- A máquina de teste é um desktop: a bateria foi verificada só por testes de mapeamento e pela galeria
  (laptop real não testado). O disco da máquina está 99% cheio, o que exerceu a cor de perigo.
- CPU é o uso total (não por núcleo); disco é só o volume do sistema; memória é a física em uso.
- O primeiro valor de CPU aparece ~1 s depois de abrir o widget (precisa de um intervalo).
- Network e o gráfico (`Chart`) seguem para o W3.
- Hashes dourados: dependem da fonte e do WARP desta máquina (`POD_UPDATE_GOLDEN=1` regrava).

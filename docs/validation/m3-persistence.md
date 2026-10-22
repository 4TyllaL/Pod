# M3 — Persistência confiável

Ambiente: Windows 11, build de debug. Testes em Windows real usaram `POD_DATA_DIR` (só em debug)
apontando para pasta temporária; o perfil real (`%LOCALAPPDATA%\Pod`) não foi tocado.

## O que existe

- `pod-model/schema`: `Workspace` JSON versionado (`schema_version` = 1, `generation` monotônica),
  validação (IDs únicos, geometria finita, limites de tamanho) e preservação de campos e tipos
  desconhecidos (`extra`).
- `storage/files`: gravação atômica (temp único → `sync_all` → `ReplaceFileW` com
  `workspace.previous.json`; rename na primeira gravação), carga com recuperação, quarentena de
  `.tmp` órfãos, trava exclusiva (`workspace.lock`, handle sem compartilhamento).
- `storage/worker`: único escritor; uma gravação em curso e no máximo o snapshot mais recente
  pendente; ack por geração; acorda a UI com `WM_APP`.
- `storage/tracker`: autosave com debounce de 500 ms e teto de 2 s; "salvo" só após ack da geração
  que contém a última revisão; falha reagenda em 5 s.
- `shell`: restaura IDs/estado/posição, exclusão só após ack, instâncias não carregáveis voltam
  intactas ao disco, snapshot final no encerramento, instância única por mutex `Local\`.

## Testes automatizados (46 em `pod-app`, 12 em `pod-model`)

Schema: roundtrip com origem negativa, campos e tipos futuros preservados, versão nova distinta de
corrupção, IDs duplicados/hex inválido/NaN/Inf/zero/negativo recusados, truncado/vazio/grande demais.
Storage: falha injetada em cada etapa (criar temp, escrever, flush, substituir), com e sem arquivo
anterior: sempre resta a última geração confirmada e nenhum temp; temp órfão preservado e nunca usado;
principal corrompido recupera do anterior preservando evidência e restaura o principal; principal
ausente; ambos inválidos preservados e quarentena sob pedido; versão nova não altera nenhum arquivo;
trava exclusiva até o `drop`; worker (ack só com arquivo em disco, falha no ack, 50 envios coalescidos
com o último sempre gravado). Tracker: debounce, teto de 2 s, ack antigo não limpa mudança nova,
ack posterior cobre geração descartada, falha → sujo → nova tentativa.

## Verificação em Windows real (automação de mouse/teclado)

| Cenário | Resultado |
| --- | --- |
| 3 cliques no botão | arquivo muda de geração 1 para 2 com estado `3` em ~1,5 s |
| Mover a janela | posição `(220,180)` gravada ao fim do movimento (geração 3); `previous` existe |
| Nova nota + `Stop-Process -Force` | reiniciar restaura 2 janelas com mesmos IDs, estados e posições |
| Segunda execução | diálogo, sai ao fechá-lo; `workspace.json` com hash idêntico |
| Excluir uma nota | janela some e o arquivo passa a ter 1 instância |
| Principal corrompido | diálogo de recuperação; evidência em `recovery/`; principal volta válido |
| `schema_version` 99 | diálogo, saída sem janelas; arquivo byte a byte idêntico |
| Tipo desconhecido, estado inválido e campos extras | voltam intactos ao disco; a instância válida é restaurada (geração 10 → 11) |

## Limitações e pendências (não fingir cobertura)

- Posição/tamanho são provisórios (pixels do desktop virtual); o M5 troca por offsets relativos à área
  útil de um monitor identificado, via migração de schema.
- O "kill" real foi um `Stop-Process -Force` em repouso; a interrupção no meio da gravação só foi
  exercitada por falhas injetadas. Queda de energia, disco cheio, acesso negado e violação de
  compartilhamento no arquivo principal não foram testados fora das injeções.
- `ReplaceFileW`: os códigos 1176/1177 (principal pode ficar ausente) foram só raciocinados; a carga
  trata principal ausente com `previous` válido, e há teste para isso.
- A trava foi testada com dois handles no mesmo processo e a instância única por mutex; duas sessões
  Windows do mesmo usuário não foram testadas.
- `Session::close` espera o worker sem limite de tempo: um disco travado atrasa a saída.
- A exclusão não pede confirmação (isso entra com Quick Notes, M4) e não há indicador visual de
  "salvo/erro"; o estado existe (`save_state`) mas não é exibido.
- Sem bandeja, Alt+F4 encerra o aplicativo e há sempre ao menos uma nota (ao abrir sem nenhuma, cria
  uma): ajustes de protótipo até o M4.
- O backup (`previous`) guarda só uma geração; recuperar dele pode desfazer a última alteração
  (por exemplo, uma exclusão), como previsto.
- Um fallback por `previous` não verifica se ele é mais antigo que o principal corrompido além da
  ordem em que ReplaceFileW os grava.
- A UI de diálogos (`MessageBoxW`) bloqueia a inicialização; só os casos de OK foram dispensados por
  automação, e o caso "iniciar espaço novo?" (Sim/Não) não foi exercitado em Windows real.
- Medições de tempo (≈1,5 s) são de uma máquina e de debug, sem repetição.

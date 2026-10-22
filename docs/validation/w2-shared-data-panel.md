# W2 — Countdown, Calendar, Reminders, dados compartilhados e painel de edição

## O que entrou

- **Dados compartilhados** (`pod-model::shared`): eventos (data + título) e tarefas, com limites (200 de cada,
  títulos de até 80 caracteres sem caracteres de controle, ano 1900–2200), ids que nunca se repetem e
  validação completa ao ler o JSON. Ficam em `extra["shared"]` do workspace (o mecanismo de campos extras
  que já existia; o schema continua 3).
- **Runtime:** guarda os dados, entrega uma cópia a cada instância (`Widget::set_shared`) e invalida todas ao
  mudar. `Effect::SharedOp(ToggleTask)` altera sem painel; `Effect::OpenPanel` pede o painel.
- **Persistência:** o shell grava os dados junto de cada snapshot e no fechamento. Se o arquivo traz dados
  que não conseguimos ler, eles ficam intactos no disco até o primeiro dado válido ser salvo.
- **Primitiva `Check`** (caixa de seleção com rótulo): layout, desenho, clique/Espaço e UI Automation
  (controle CheckBox, nome "Tarefa, pendente/concluída", `Invoke`).
- **Painel de edição** (`panel.rs`): janela com **controles nativos** do Windows (lista, campos, botões), então
  Tab, Enter (Adicionar), Esc (fechar), IME e fonte do sistema funcionam sem código próprio. Eventos: data
  `dia/mês/ano` e título; tarefas: título, alternar concluída, remover. Erros aparecem no painel ("Digite um
  título.", "Data inválida…") e o foco volta ao campo com problema. Um painel por tipo; reabrir foca o existente.
  Mudanças feitas por um widget (marcar uma tarefa) atualizam a lista aberta.
- **Widgets** (1×1, 2×1, 2×2): `pod.calendar` (data e próximo evento; grade do mês com hoje destacado e
  dias com evento em cor de acento), `pod.countdown` (dias até o próximo evento ou um escolhido no menu;
  "Hoje", "Amanhã", "N dias atrás"), `pod.reminders` (caixas clicáveis, contador no 1×1, "+N mais",
  "Ocultar concluídas"). Menu de cada um: "Editar eventos…" / "Editar tarefas…".
- Galeria: 156 imagens (52 casos × 3 temas) com dados de demonstração compartilhados; goldens regravados.

## Verificado em janela real (debug)

| Ação | Resultado |
|---|---|
| Restaurar `shared` de um workspace e abrir os 3 widgets | tarefas, "2 dias · Reuniao · 20/10/2026" e a grade do mês corretos; UIA lê "Comprar pao, pendente" etc. |
| Clique real na caixa de uma tarefa | vira "concluída" e o disco passa a ter `done:true` (10 cliques seguidos alternaram 10 de 10 vezes) |
| Menu → "Editar tarefas…" | abre o painel (1 janela `PodPanel`); reabrir não duplica |
| Digitar "Estudar Rust" + Enter | a tarefa aparece na lista do painel e no widget; gravada no disco |
| Enter com campo vazio | "Digite um título." |
| Selecionar uma linha e "Alternar concluída" | widget e disco atualizam |
| Esc | painel fecha |

Testes: 149 no `pod-app` e 90 no `pod-model` (dados compartilhados e validação, aritmética de datas,
grade do mês, widgets em todos os formatos e com listas longas, checkbox no layout/UIA).

## Limites e observações

- O painel usa controles clássicos do Windows (visual diferente dos cartões). O DPI é o do monitor onde abriu;
  mover o painel para um monitor de outro DPI não reflui os controles (não testado).
- UI Automation do painel: os controles aparecem como "Pane" com a classe do controle na leitura feita por
  PowerShell; o teste foi por mouse e teclado reais, não por leitor de tela.
- Sem edição de um evento existente (remover e adicionar de novo); sem repetição/lembrete com horário.
- Countdown mostra dias, não horas; a data muda à meia-noite (assinatura de hora por minuto).
- Dados compartilhados ilegíveis no arquivo: o Pod segue com a lista vazia e não regrava por cima até haver
  uma edição válida — nesse momento o conteúdo antigo ilegível é substituído (não há cópia separada).
- Sincronização entre dispositivos e importação de calendários não existem.
- Ainda não feitos do W2 do ADR: Drink water, Converter, Favorites, Parcels.

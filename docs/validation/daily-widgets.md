# Widgets do dia a dia: Hábitos, Agenda, Atalhos

- **Hábitos** (`pod.habits`, `pod-model/habits.rs`): marcar o que cumpriu hoje, sequência de dias seguidos
  (a de ontem vale até marcar hoje), até 12 hábitos, 120 dias guardados por hábito. Adicionar pelo botão/menu,
  remover pelo menu. Formatos 1×1 a 2×4. Estado durável no próprio widget.
- **Agenda** (`pod.agenda`): lista os eventos do Calendário agrupados por dia (Hoje, Amanhã, data).
  Usa os mesmos dados compartilhados; editar pelo menu abre o painel de eventos. 1×1 a 2×4 e 4×1/4×2.
- **Atalhos** (`pod.launcher`, `pod-model/launcher.rs`, `pickers.rs`): botões que abrem programas e pastas.
  O host mostra o seletor de arquivo/pasta do Windows e devolve só o caminho; na hora de abrir revalida
  (absoluto, só `.exe`/`.lnk` ou pasta, existe, tipo certo, sem argumentos). Scripts (.bat, .ps1…) recusados.
- Anotações rápidas já existiam (notas); não foram refeitas.

## Não verificado
- Os seletores do Windows (arquivo/pasta) e a abertura real não foram acionados ao vivo (diálogo modal);
  há testes da validação e do recuso, e modo simulado (`POD_DRY_RUN_LINKS`) em depuração.
- Clique real do mouse nos botões dos cartões atrás de janelas segue como nos outros widgets.

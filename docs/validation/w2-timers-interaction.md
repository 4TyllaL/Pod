# W2 — Timer, Stopwatch, Pomodoro e interação nos cartões

## O que entrou

- **Interação:** botões nos cartões (`Kind::Button`) recebem clique do mouse (a janela é arrastável em qualquer
  ponto, exceto sobre um botão), **Espaço/Enter** aciona o primeiro botão da view, e o **UI Automation**
  expõe cada botão com `Invoke`. A árvore de acessibilidade continua gerada da view.
- **Tempo nos eventos:** `Widget::set_now` — o runtime informa o relógio monotônico antes de cada evento,
  então cliques e comandos de menu podem datar a ação sem mudar `Event::Clicked`.
- **Aviso sonoro:** `Effect::Chime` (`MessageBeep`), com opção "Som ao terminar" por instância.
- **Ajustes:** `MenuItem` ganhou `group`; itens do mesmo grupo viram **submenu** no menu de contexto
  (Duração; Foco, Pausa curta, Pausa longa, Rodadas; Período). Os valores vêm de listas fixas e são
  validados também ao restaurar o estado.
- **Lógica pura** (`pod-model::timekeeping`): `Countdown`, `Stopwatch`, `Pomodoro`, `fmt_hms`; tudo em `now_ms`.
  O mostrador arredonda para cima e o agendador acorda exatamente quando o valor mostrado muda
  (um despertar por segundo, só enquanto roda).
- **Widgets** (1×1, 2×1, 2×2): `pod.timer`, `pod.stopwatch`, `pod.pomodoro`; bandeja → "Novo widget".
  Pomodoro: foco → pausa curta → … → pausa longa na última rodada; ao fim de cada fase toca o aviso e
  espera o usuário ("Próxima").
- Galeria: 129 imagens (43 casos × 3 temas); goldens regravados.

## Verificado em janela real (debug, mouse e teclado reais)

| Ação | Resultado |
|---|---|
| Clique em "Seguir" do timer (4 s restantes) | vira "Pausar" e conta; ~5 s depois mostra `00:00`, "De novo" |
| Clique em "Iniciar" do cronômetro; esperar 3 s | `00:03`; "Volta" mantém rodando; "Pausar" → `00:06`, "Seguir" |
| Clique fora dos botões + Espaço (pomodoro) | "Iniciar" → "Pausar" → "Seguir" (segundo Espaço pausa) |
| UIA `Invoke` em "Zerar" | cronômetro volta a `00:00`, "Iniciar" |
| Persistência | timer concluído grava `remaining_ms:null`; pomodoro pausado grava ~1 499 4xx ms restantes |
| Menu de contexto do pomodoro | ações, submenus Foco / Pausa curta / Pausa longa / Rodadas, "Som ao terminar", "Tamanho" |

Testes: 133 no `pod-app` e 80 no `pod-model` (máquinas de estado, agendamento por segundo, restauração e
rejeição de estado inválido, voltas, fases, todos os formatos cabem no cartão e o primeiro botão é a ação
principal, retângulos de botões, formatação).

## Limites e decisões

- **Ajustes por menu, não por painel próprio.** Não existe ainda uma janela/painel de ajustes nem entrada
  de texto fora das notas; por isso as durações são listas fixas em submenus. Durações livres (ex.: 37 min)
  exigem o painel padrão do ADR, que fica para quando houver campo de texto em cartão.
- **Timer em andamento não sobrevive ao fechamento do Pod:** volta pausado com o tempo restante (não há
  relógio de parede no evento; o ADR previa os dois). Tempo de suspensão do Windows pode não contar no
  relógio monotônico: **não testado**.
- Sem hover/pressionado nos botões (só o clique); sem foco visível por Tab entre botões — o teclado cobre
  só a ação principal (Espaço/Enter) e o menu de contexto.
- Fim de timer: aviso sonoro do sistema e estado "De novo"/"Próxima"; não há notificação de bandeja nem
  destaque com a janela escondida (ADR, item 5, fica para o W4).
- Os pontos de rodada do Pomodoro usam ● e ○, que a fonte desenha com tamanhos um pouco diferentes.
- Não medi custo de CPU com muitos timers rodando; o desenho é por segundo e só em cartões que mudam.
- Countdown/Calendar/Reminders/Drink water/Converter/Favorites/Parcels (restante do W2 do ADR) não foram feitos.

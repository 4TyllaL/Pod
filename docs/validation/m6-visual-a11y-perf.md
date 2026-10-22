# M6 — Qualidade visual, acessibilidade e desempenho

Ambiente de medição: Windows 11 Pro build 26200; AMD Ryzen 5 5600G (12 processadores lógicos);
GPUs AMD Radeon Graphics (driver 31.0.12027.9001) e Radeon RX 7600 (32.0.31041.1004); três monitores
1920×1080 a 100% DPI; build **release (opt-level 3) com asserts de debug ligados** (para manter o
redirecionamento de dados de teste); sem WARP, sem RDP. Dados de teste em `POD_DATA_DIR`; o perfil real
não foi tocado. Uma máquina, sem repetição estatística.

## Direção visual (referências do usuário) e o que foi adotado

Adotado, dentro do escopo do MVP: cartões com raio de 14 DIP; superfície opaca com gradiente vertical
sutil; contorno de 1 DIP e realce interno no topo; faixa de movimentação tingida pelo acento da nota, com
ponto de acento; tipografia com hierarquia (Segoe UI Variable Text, instalada nesta máquina, com fallback
para Segoe UI; 13 DIP interface, 11 DIP status, 12 pt no editor); temas claro, escuro e alto contraste;
botão de menu "⋯"; transições curtas de hover e foco.

**Não adotado (decisões de arquitetura mantidas):** transparência/blur de fundo ("vidro" real) fica
pós-MVP (ARCHITECTURE §6): o brilho é um gradiente e uma linha de realce sobre superfície opaca. Galeria
com +/−, tamanhos pequeno/médio/grande, grade com snapping e os outros tipos de widget das referências
(relógio, calendário, clima, bateria etc.) ficam para depois do MVP, com ADR.

## O que existe

- `ui/theme`: tokens de cor para claro, escuro e alto contraste (cores do sistema, sem gradiente, tinta
  nem realce, contorno de 2 DIP) e a conta de contraste WCAG; `sysinfo` lê tema do aplicativo
  (`AppsUseLightTheme`), alto contraste e redução de animações. O tema muda em `WM_SETTINGCHANGE`
  ("ImmersiveColorSet" ou alto contraste), atualiza a cor do texto do editor e redesenha.
- `ui/anim`: tween ease-out de 150 ms, sem salto ao redirecionar; com "reduzir animações" vai direto ao
  alvo. Quadros (timer de ~16 ms) só existem enquanto há transição.
- Foco lógico por nota (editor ↔ botão de menu): Esc sai do editor (nunca em composição IME), F6 alterna,
  Tab volta do botão (no editor, Tab é texto), Enter/Espaço no botão, Shift+F10 e tecla de menu abrem o
  menu (ao soltar a tecla; F10 é tecla de sistema). Anel de foco no botão; contorno migra para o acento
  quando o editor está em foco.
- UI Automation: além do `Edit` "Nota" (ValuePattern), o botão "Menu da nota" (InvokePattern, foco,
  retângulo) e a navegação entre irmãos.
- `metrics`: contadores de quadros e percentis do trabalho de desenho (BeginDraw..EndDraw, sem o `Present`).

## Testes automatizados (71 em `pod-app`, 31 em `pod-model`)

Contraste WCAG medido por teste em claro, escuro e alto contraste: texto/superfície ≥ 7:1, texto
secundário ≥ 4,5:1, rótulo sobre cada acento ≥ 4,5:1, anel de foco/acento sobre a superfície ≥ 3:1, texto
sobre a faixa tingida ≥ 7:1 (o teste reprovou a tinta do escuro e ela foi reduzida). Tween (monotonia,
término exato, sem salto, movimento reduzido, no-op), geometria do botão de menu, percentis e janela
limitada de métricas.

## Verificação em Windows real

| Cenário | Resultado |
| --- | --- |
| Temas claro/escuro/alto contraste (forçados em debug) e foco | capturas conferidas: cartão arredondado, faixa tingida, contorno no acento com editor focado, alto contraste com contorno grosso e sem tinta |
| Hover no botão "⋯" | pixel do botão 59 → 80 aos 50 ms → 84 em regime → 59 ao sair |
| Teclado | Esc/F6/Tab movem o foco lógico; digitar com o botão focado não altera o texto; Tab no editor insere tabulação; Enter no botão, Shift+F10 e `Invoke` via UIA abrem o menu; Esc fecha |
| UIA (restrito à janela do Pod) | `Edit` "Nota" e `Button` "Menu da nota" com ids, focáveis, retângulo do botão correto, `HasKeyboardFocus` acompanha o foco lógico |
| Regressões | digitar/undo/colar/limite/Esc/exclusão confirmada, 10 notas independentes e restauração, placement (simulado), energia, perda de dispositivo e tela cheia continuam passando |

## Métricas (10 notas de 320×240 DIP, build acima)

| Item | Meta da arquitetura | Medido |
| --- | --- | --- |
| CPU em repouso, 60 s, nenhuma nota em foco | < 0,5% da máquina | 0,000% (0,00 s de CPU em 60 s); 0,02–0,07% em rodadas em que uma nota estava em foco |
| Quadros apresentados em repouso | nenhum | **0** em 60 s sem foco; 19 em 10 s com um editor focado (caret piscando); 0 em 5 s após Esc |
| Memória privada | < 150 MiB | 36,1 MiB (working set 45,7 MiB); memória de GPU local 23,4 MiB (contador do sistema) |
| p95 do trabalho de UI em resize contínuo | < 8 ms | p50 0,34 ms, p95 0,50 ms, máx 1,69 ms (341 amostras); p95 total com `Present` 1,18 ms |
| 60 ciclos criar/excluir (após 5 de aquecimento) | sem vazamento | handles 1187→1185, GDI 14→14, USER 41→41, memória privada 37,1→37,6 MiB |

## Defeitos encontrados e corrigidos neste milestone

- **Catch-all no `match` do WndProc.** `WM_MOUSELEAVE` não estava importado e virou uma variável de
  padrão que engolia toda mensagem seguinte (cliques, timers, fechar). O compilador avisava
  ("unreachable pattern"); agora o crate usa `#![deny(unreachable_patterns)]`.
- **Quadros contínuos em repouso.** Cada editor, mesmo sem foco, ativava o caret e seu timer de piscar:
  ~19 apresentações/s com 10 notas. O caret agora só existe e pisca com foco.
- Menu aberto no pressionar da tecla fechava ao soltá-la (e F10 é `WM_SYSKEYDOWN`): passou a abrir no soltar.

## Limitações e pendências (não fingir cobertura)

- **Narrator não foi executado.** Só um cliente UI Automation. Falta `TextPattern` (seleção, caret, leitura
  por linha), eventos de mudança de valor e de foco, e o nome acessível da nota. Narrator é o critério do M1.
- **Alto contraste só foi visto forçado pelo atalho de debug**, com as cores do sistema do esquema normal;
  não testei um tema de alto contraste real nem a troca ao vivo pelo sistema. O tema claro/escuro também só
  foi alternado por debug: a reação à mudança real das configurações do Windows (`WM_SETTINGCHANGE`
  "ImmersiveColorSet") **não foi exercitada**.
- **"Reduzir animações"** está coberto por teste de regra e pela leitura de `SPI_GETCLIENTAREAANIMATION`,
  mas não alternei a configuração do sistema.
- Navegação completa por teclado **incompleta**: mover e redimensionar notas só funciona com o mouse (a
  janela sem moldura não tem menu de sistema); não há Tab entre notas diferentes.
- Visual conferido só a 100% DPI; layouts e fonte a 150/200% e DPI misto não foram vistos.
- Emoji restaurado do disco continua como caixa (pendência do M1/M4); o botão de menu é desenhado com
  círculos, sem depender de fonte.
- O caret piscando redesenha o cartão inteiro (≈2 quadros/s por editor focado); um retângulo sujo reduziria
  isso, e a meta de repouso exclui explicitamente o caret.
- Medições: uma máquina, build release com asserts de debug, sem repetição, sem WARP/RDP; memória de GPU
  vem do contador "GPU Process Memory", não de uma ferramenta de GPU; nenhum teste ETW/WPR.
- Atalhos de teste Ctrl+Shift+T/R/L/K e `POD_DATA_DIR` só existem com asserts de debug.

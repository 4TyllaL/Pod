# Clicar num cartão deixa "nenhuma janela" em primeiro plano? (app real)

Data: 21 de outubro de 2026. Windows 11 build 26200, 3 monitores a 96 DPI, build de debug. Investigação pedida depois de o controle das provas
da sombra mostrar esse comportamento (ver [shadow-s56.md](shadow-s56.md), achados paralelos).

## Resposta

**Sim, no app real.** Com outro programa em primeiro plano e o cartão visível sobre a área de trabalho (modo normal, camada desktop), um
**clique real** num cartão:

- **deixa nenhuma janela em primeiro plano** (`GetForegroundWindow()` devolve `NULL`) em 4 de 4 cliques;
- **desativa o outro programa**: ele registra `form DESATIVADO` e o campo de texto `PERDEU foco`;
- uma tecla digitada logo depois **não chega** ao outro programa (o teclado fica sem destino até o usuário clicar nele de novo).

Isso contradiz a intenção do desenho ("cartão não ativa ao clicar: não rouba o foco"): o cartão não se ativa, **mas o programa que
estava em uso perde o foco do mesmo jeito**.

## Como foi medido (reproduzível)

- `tools/verify-focus-click.ps1`: sobe o **app real** com dados isolados (`--data-dir`, um relógio; **o workspace do usuário não é
  tocado**), acha um trecho da área de trabalho **sem nenhuma janela por cima** (a janela sob o ponto e a raiz precisam ser da área de
  trabalho), põe o cartão ali e abre uma janela de **outro programa em outro processo** (`tools/focus-helper.ps1`, Windows Forms) que
  registra ativação, foco do campo de texto e o que foi digitado. Faz **cliques reais** (`mouse_event`; o cursor é restaurado depois).
- Controles: clicar **no próprio outro programa** (o foco fica e a tecla chega) e clicar **na área de trabalho vazia** (o foco vai para a
  área de trabalho, o que é o normal do Windows). Só o clique no cartão deixa o foco sem janela.

## Causa: `WS_EX_NOACTIVATE`

`tools/verify-focus-variants.ps1` repete o clique com o **cartão da prova** (que copia o contrato do host e já tinha mostrado o mesmo
comportamento), mudando **um detalhe por vez** (`pod-app --focus-card <variante> <x> <y> <segundos>`):

| Variante do cartão | Cliques | Outro programa **manteve** o primeiro plano | "Nenhuma janela" |
| --- | --- | --- | --- |
| **igual ao host** (`MA_NOACTIVATE`, `WM_NCACTIVATE`=1, `SetCapture`) | 16 | **0 de 16** | **13 de 16** (os outros foram para a área de trabalho/Chrome) |
| sem `SetCapture` | 6 | 0 de 6 | 6 de 6 |
| `WM_NCACTIVATE` padrão | 6 | 0 de 6 | 4 de 6 (2 foram para o Chrome) |
| **com `WS_EX_NOACTIVATE`** | 16 | **16 de 16** | **0 de 16** |
| `WS_EX_NOACTIVATE` e sem `SetCapture` | 6 | 4 de 6 (2 foram para o Chrome: provável interação do usuário durante o teste) | 0 de 6 |
| controle: ativação padrão | 6 | 0 de 6 | 0 de 6 (o cartão virou o primeiro plano em 6 de 6) |

- `SetCapture` e `WM_NCACTIVATE` **não são a causa**.
- `MA_NOACTIVATE` sozinho **não basta**: ele impede o cartão de se ativar, mas o Windows desativa a janela em foco de qualquer forma.
- Com `WS_EX_NOACTIVATE` na criação do cartão, o outro programa **manteve o foco em todos os 16 cliques**, sem evento de desativação
  (0 de 13 medidos).

## Correção candidata (aplicada depois; ver a seção final)

Criar os **cartões de formato fixo** (não as notas, que ativam para digitar) com `WS_EX_NOACTIVATE`, na criação da janela em
`host.rs` (hoje `WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP`).

**Risco a verificar antes de aplicar:** o teclado. O DESIGN prevê foco por teclado, F6 e Invoke por UI Automation nos cartões; uma janela
`WS_EX_NOACTIVATE` não é ativada e pode não receber o foco do teclado do mesmo jeito. Precisa de teste de teclado e do Narrator.

## Limites

- **Uma máquina, um monitor-alvo** (área de trabalho livre do monitor da esquerda), build de debug, cartão `2x1` de relógio.
- Rodei com o seu mouse e a sua tela em uso: o cursor se move rapidamente e volta; em duas execuções houve ruído (o Chrome apareceu como
  primeiro plano em algumas tentativas, o que atribuo à sua interação).
- A medida "a tecla chegou ao outro programa" **não é confiável nos casos em que o foco foi mantido**: o meu truque de Alt para trazer a
  janela de teste à frente deixa o menu da janela em espera e engole a primeira tecla (um Esc mitiga, mas não eliminou). Por isso a
  conclusão se apoia no **primeiro plano e nos eventos de desativação**, não na tecla.
- O **modo de edição** (cartões `TOPMOST`) e as **notas** (que ativam por design) não foram medidos aqui.

## Correção aplicada e verificada (mesma data)

**Mudança:** `host.rs` agora cria os **cartões de formato fixo** com `WS_EX_NOACTIVATE` (`window_ex_style(chrome, review)`, com teste unitário
`ex_style_tests`). Notas e janelas de revisão não mudam. `WM_MOUSEACTIVATE` continua devolvendo `MA_NOACTIVATE`.

**Teste** (`tools/verify-tile-keyboard.ps1`, app real com dados isolados, cartão de água com botões `+250` e `−` e uma nota, outro programa em
outro processo, UI Automation gerenciada de verdade, cliques e teclas reais). Antes e depois:

| Verificação | Antes | Depois |
| --- | --- | --- |
| Estilo `WS_EX_NOACTIVATE` no cartão / na nota | ausente / ausente | **presente / ausente** |
| **Clique real no cartão**: o outro programa mantém o primeiro plano | **0 de 5** (5 de 5 sem janela, 5 de 5 desativações) | **5 de 5** (0 sem janela, 0 desativações) |
| **UIA `Invoke`** no botão `+250` (0% → 12%) | funciona | funciona (igual) |
| UIA `Invoke` com outro programa em foco: reage e o outro programa continua em primeiro plano | sim / sim | sim / sim |
| **Espaço** no cartão depois de um clique real | não reage; o foco ia para "nenhuma janela" | não reage; **a tecla vai para o outro programa, que tem o foco** |
| UIA `SetFocus` na raiz do cartão, depois Espaço | erro "o elemento de destino não pode receber o foco"; não reage | **igual** (erro igual; não reage) |
| **Nota**: clicar ativa e **F6** alterna editor ↔ botão "Menu da nota" | ativa; alterna | ativa; alterna (**igual**, nas 4 execuções "depois") |
| **Menu de contexto** do cartão (botão direito): abre, **Esc** fecha, **clique fora** fecha, seta para baixo + Enter | abre / fecha / fecha / fecha o menu | **igual** em tudo |
| **Arrastar** o cartão com o mouse (captura de mouse) | n/a (medido só depois) | moveu (82, 82 px, encaixado na grade); o outro programa manteve o primeiro plano |

"Depois" foi rodado **4 vezes** (3 com o código antes da refatoração para `window_ex_style` e 1 com o código final), todas com os mesmos resultados.

### O que isso diz sobre teclado, F6 e UI Automation

- **F6 só existe nas notas** (alterna editor e botão de menu): não foi tocado e continua funcionando. **Os cartões nunca tiveram F6.**
- **UI Automation:** o `Invoke` dos botões do cartão **não depende de ativação** (posta `WM_TILE_INVOKE`) e continua funcionando, inclusive com outro
  programa em foco.
- **Espaço/Enter no cartão** (o primeiro botão) existe no código, mas **o cartão nunca recebia o foco do teclado no uso real**, nem antes:
  `MA_NOACTIVATE` impede a ativação por clique, e o UIA `SetFocus` na raiz **já falhava** ("o elemento de destino não pode receber o foco", igual
  antes e depois). A correção **não regride isso**, e o efeito positivo é que, depois de clicar num cartão, o teclado **continua com o programa em uso**.
- **Lacuna de acessibilidade que já existia e continua:** **não há como colocar o foco do teclado num cartão** (nem pelo mouse nem pelo UIA
  `SetFocus`). Quem usa só teclado ou leitor de tela aciona os botões pelo `Invoke` do UIA, mas não por Espaço/Enter. Resolver isso é outra
  decisão (por exemplo, `SetFocus` dos itens/raiz ativando o cartão de propósito, o que conflita com "não ativar"; ou um atalho global).

### Limites

- **Não testado:** **modo de edição** (cartões acima das outras janelas, selos "−"/"+") e o menu da **bandeja**, que também usam os cartões; o **Narrator**
  de verdade; cartões **cobertos por outras janelas** (só testei na área de trabalho livre); mais de um cartão de tipos diferentes (um cartão de
  água) e DPI diferente de 96.
- **O menu de contexto** do cartão ainda chama `SetForegroundWindow` no próprio cartão antes do `TrackPopupMenu` (necessário para o menu
  fechar ao clicar fora): **ele continua pegando o primeiro plano enquanto o menu está aberto**, antes e depois da correção. Não mudei isso.
- A seleção por teclado no menu (seta para baixo + Enter) fechou o menu mas não alterou o cartão, **antes e depois igual**; não investiguei qual item é o primeiro.
- Uma máquina, um monitor-alvo (esquerda), debug; a tela do usuário em uso (o cursor se move e volta). Em uma rodada "antes" a nota não ativou (tropeço
  do teste, sem relação com a mudança: a build estava revertida naquele momento).

## Modo de edição e menu da bandeja com os cartões sem ativação (mesma data)

Teste: `tools/verify-edit-tray.ps1` (app real, dados isolados, um cartão de água, um relógio e outro programa em outro processo). Abre o menu da
bandeja de verdade (mensagem da bandeja para a janela de controle), escolhe os itens **clicando neles** (retângulo do item informado pelo Windows),
arrasta e redimensiona com o mouse e exclui pelo menu de contexto. Rodado **com e sem** `WS_EX_NOACTIVATE`: **os resultados foram idênticos em todas as medidas**.

| Verificação | Antes | Depois |
| --- | --- | --- |
| Menu da bandeja abre; itens: Nova nota, Novo widget, Editar widgets…, [cartões: Água, Relógio], Ocultar todas, Vidro, Widgets de internet, Sair | sim | igual |
| Clicar fora fecha o menu da bandeja | sim | igual |
| **Editar widgets** (bandeja): galeria Pod Studio abre; cartões viram `TOPMOST`; a galeria toma o primeiro plano | sim | igual |
| Em edição o cartão continua com/sem `WS_EX_NOACTIVATE` (ausente / **presente**), e o estado não muda na edição | ausente | presente |
| **Arrastar** o cartão inteiro em edição (mouse): move (-1410,30 → -1308,112) e o primeiro plano continua na galeria | sim | igual |
| **Puxar a borda** do cartão de água: muda de formato (188×188 → 388×188) | sim | igual |
| Item de edição na bandeja fica **marcado** em edição; escolher de novo **desliga** a edição, fecha a galeria e tira o `TOPMOST` | sim | igual |
| **Esc** na galeria sai da edição; cartões sem `TOPMOST` e abaixo das outras janelas (ordem Z) | sim | igual |
| **Ocultar todas**: some os cartões; de novo mostra | sim | igual |
| **Excluir widget…** no menu de contexto de um cartão sem conteúdo: apaga na hora, sem confirmação, e **grava o workspace sem ele** | sim | igual |
| **Revelar** o relógio escolhendo-o na lista da bandeja, com o outro programa **cobrindo** o cartão: o cartão fica na frente do outro programa | **não** (primeiro plano = janela de controle) | **não** (igual) |

### Achados

- **Não há selos "−" nos cartões** no código atual. `gfx::Surface::draw_tile` diz: "Sem selo de excluir nem alça: excluir fica no menu do botão direito e o
  tamanho muda puxando as bordas do cartão". O selo (`render::paint_badge`) só existe nas **miniaturas da galeria**. Portanto o "selo −" do ADR-0003 P4 e das
  notas de validação antigas **não existe mais**; o que há para testar em edição é arrastar, puxar a borda, a galeria e o menu de contexto, e foi testado.
- **Excluir cartão não tem janela de confirmação**: `Shell::request_remove` só pergunta se o widget "tem conteúdo" (`has_content`), e **só as notas** implementam isso.
  Logo o `MessageBox` com um cartão como dono (que era uma preocupação com `WS_EX_NOACTIVATE`) **nunca é usado por cartões**.
- **Revelar um cartão coberto não o traz para a frente**, **antes e depois igual**. Coerente com a regra da camada desktop (cartões não sobrepõem os aplicativos), mas
  se a intenção do item da bandeja era "mostrar e trazer", não acontece com cartões. Sem relação com a correção.
- **Esc no menu da bandeja** não pôde ser testado: o teclado só chega ao menu se o app tomar o primeiro plano, e nesta forma de abrir o menu (mensagem, sem um
  clique real no ícone) a janela de controle não conseguiu (o primeiro plano ficou com o outro programa). Os cliques funcionam; o ponto é uma limitação do teste.

### Limites

- Não testei: **o botão "+" das miniaturas da galeria** (adicionar widget; a galeria é desenhada sem UI Automation e eu não tenho as coordenadas), **a bandeja pelo ícone real**
  (a mensagem é a mesma que o ícone envia, mas o Explorer não participou), o submenu **Novo widget**, **Vidro** e **Widgets de internet**, a **exclusão de uma nota** (ativa e
  tem confirmação), mais de um monitor para os cartões em edição, e DPI diferente de 96.
- Uma máquina, um monitor-alvo, debug; o cursor se mexe durante o teste. A janela de teste fica em área de trabalho livre, então não exercita cartões cobertos por aplicativos
  além do caso do `reveal`.

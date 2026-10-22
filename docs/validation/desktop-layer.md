# Camada desktop: widgets no fundo, nunca sobre os aplicativos

Pedido do dono: os widgets não podem sobrepor a tela; precisam ficar no desktop. Implementa o essencial da
P2 do ADR-0003 (`Desktop` como camada padrão).

## Regras

- **Cartões e notas ficam atrás de todas as janelas normais**, logo acima do desktop (Progman). Só
  operações documentadas: `HWND_BOTTOM` e a regravação do `hwndInsertAfter` no `WM_WINDOWPOSCHANGING` das
  próprias janelas. Nenhum laço disputa a ordem com outros processos (ARCHITECTURE §5).
- Criar um widget **não o levanta mais**; ele nasce no fundo (`WM_SHOWWINDOW`).
- **Cartão não ativa ao clicar** (`WM_MOUSEACTIVATE` → `MA_NOACTIVATE`) e o arrastar pela "legenda" inicia o
  movimento direto (`SC_MOVE`), sem a ativação padrão: não rouba o foco do aplicativo do usuário.
- **Notas** vêm à frente só para digitar (nova nota ou escolhida na bandeja) e **voltam ao fundo ao perder o
  foco** para outra janela.
- **Modo de edição** (ADR-0003 P4): os cartões ficam acima dos aplicativos enquanto se edita, para ver a
  grade e adicionar/remover; ao sair voltam ao fundo e perdem o estado de "sempre no topo".

## Verificado ao vivo (release, 150% de DPI, desktop com vários aplicativos abertos)

| Verificação | Resultado |
|---|---|
| Ordem Z fora da edição | 13 janelas visíveis acima do cartão; abaixo dele só `SysShadow` e `Progman` |
| Tentativa externa de levar ao topo (`SetWindowPos HWND_TOP`) | o cartão continuou abaixo da janela comum de teste (pixel magenta da janela, não do cartão) |
| `WM_MOUSEACTIVATE` enviado ao cartão | devolve 3 (`MA_NOACTIVATE`) |
| Entrar na edição pelo menu | índice Z 13 → 2 e estilo `WS_EX_TOPMOST` ligado |
| "Concluído" | volta ao índice 13 e `WS_EX_TOPMOST` desligado |

## Não verificado / limites

- O **clique real do mouse** num cartão visível não foi reproduzido: com o cartão atrás dos aplicativos do
  usuário, nenhum ponto dele ficou à mostra para clicar. A garantia vem do `MA_NOACTIVATE`, do `SC_MOVE` direto
  e da regravação do Z; o caminho `WM_NCLBUTTONDOWN` → `SC_MOVE` (arrastar) não foi exercitado ao vivo.
- **Acessibilidade de teclado dos cartões muda**: sem ativação por clique, o cartão não recebe foco do teclado
  (Espaço/Enter no primeiro botão). O UI Automation `Invoke` e o menu da bandeja continuam funcionando.
- **Win+D** segue o comportamento do Windows (ARCHITECTURE §5): não garanti visibilidade após "Mostrar
  área de trabalho". Também não testei com um aplicativo em tela cheia nem com vários monitores.
- Ficam fora desta etapa o resto da P2 (`Overlay` e `Popup` por widget) e as camadas por instância.

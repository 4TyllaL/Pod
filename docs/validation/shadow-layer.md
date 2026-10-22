# Prova S2 da sombra ampla: camada desktop, arrastar e redimensionar (ADR-0005, etapa 4)

Data: 21 de outubro de 2026. Windows 11 build 26200, um monitor, escala 100%, **build de debug**. Código:
`crates/pod-app/src/shadow_layer.rs`, rodado com `pod-app --shadow-layer <pasta>` (isolado: não cria cartões do
produto, não mexe em workspace nem na bandeja). Relatório da execução 1: [shadow-layer-report.md](shadow-layer-report.md).
**3 execuções seguidas, todas com todas as verificações passando e código de saída 0.**

> **Correção posterior (mesma data).** Na primeira versão desta prova a janela normal de teste era criada com `WS_VISIBLE`, o que a
> ativava e **tirava o foco do usuário**; a verificação "primeiro plano não mudou" comparava com uma base tirada depois disso, então ela
> não provava que os cartões e a sombra não mexem no foco. Corrigido: a base agora é o primeiro plano **antes de criar qualquer janela** e a
> prova foi refeita (3 execuções, todas passando). Resultados do foco depois da correção: **criar cartões, sombras e a janela normal não
> muda o primeiro plano**; um clique real num cartão deixa "nenhuma janela" em primeiro plano **com e sem a sombra** (o controle é igual, então
> não vem da sombra; ver [shadow-s56.md](shadow-s56.md), achados paralelos). A frase "sem roubar foco" da conclusão vale só assim: a sombra
> não altera o foco. Também foi achado que a nota ignorou `HWND_TOPMOST` depois do redimensionamento real (ver o mesmo documento).

## O que foi reproduzido do host

A prova copia o contrato da camada desktop de `host.rs`:

- `WM_WINDOWPOSCHANGING` **sem `SWP_NOZORDER`** vira `HWND_BOTTOM` (salvo modo de edição ou `RAISE_OK`);
- `WM_SHOWWINDOW` manda o cartão para o fundo; `WM_MOUSEACTIVATE` devolve `MA_NOACTIVATE`; `WM_NCCALCSIZE` zera a moldura;
- janelas `WS_POPUP | WS_THICKFRAME`, cartões em DirectComposition (`gfx::Surface`, `WS_EX_NOREDIRECTIONBITMAP`);
- **cartão de formato fixo:** arrasta por captura de mouse (`SetCapture` + `SetWindowPos` com `SWP_NOZORDER`), como
  `begin_drag`; **nota:** redimensiona pela borda inferior direita com o laço modal do sistema (`HTBOTTOMRIGHT`) e
  arrasta pela legenda via `WM_SYSCOMMAND 0xF012`, como o host;
- modo de edição: cartões `TOPMOST` (como `set_edit_layer`) e volta ao fundo (`HWND_BOTTOM`).

Para a sombra: uma janela `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` por cartão,
com o **mesmo hook de z**, que acompanha posição, tamanho e ordem do cartão. Em cada mudança de z do cartão,
`restack()` põe todas as sombras no fundo (ou, em edição, logo abaixo do cartão mais baixo), para que **nenhuma sombra
fique acima de um cartão vizinho**. A imagem da sombra vem de **nove fatias** (cantos e bordas copiados de uma base
pequena, miolo repetido), não de um desfoque novo a cada tamanho.

## Cenário

6 cartões de 88 DIP (grade 3×2) + 1 nota de 200×140 DIP, mais uma janela normal (não `TOPMOST`) sobre a parte de baixo
dos cartões do meio, que faz o papel de "um aplicativo do usuário".

## Resultado (41 verificações por execução, 0 falhas nas 3 execuções)

| Passo | Resultado |
| --- | --- |
| 1. Depois de criar | sombras abaixo de todos os cartões; cartões e sombras abaixo da janela normal; retângulo da sombra = cartão ± margem; foco inalterado |
| 2a. `HWND_TOP` no cartão **e na sombra** (sem ativar) | o hook reescreve para o fundo: invariantes mantidos |
| 2b. `BringWindowToTop` no cartão | ordem mantida; **a ativação ocorre** (ver abaixo) |
| 3. Clique real (`SendInput`) no cartão | cartão não sobe, foco inalterado |
| 4. **Arrastar de verdade** (`SendInput`, captura de mouse), 12 passos | cartão moveu exatamente (140, 90); a sombra acompanhou **em cada passo (12 atualizações, 0 divergências)**; ordem e foco mantidos |
| 5. Mudança de formato (`SetWindowPos` de tamanho: 88×88 → 188×88 → 188×188 → 88×88) | tamanho certo; sombra com imagem nova a cada formato; ordem e geometria mantidas |
| 6. **Redimensionar a nota pelo canto de verdade** (laço modal, `SendInput`), 14 passos | nota cresceu; a sombra acompanhou **o tamanho em cada passo (14 imagens novas, 0 divergências)**; ordem mantida |
| 7a. Modo de edição ligado | cartões e sombras acima da janela normal; **sombras abaixo de todos os cartões** |
| 7b. Modo de edição desligado | tudo de volta ao fundo, abaixo da janela normal |

### Informativos (não são falhas da sombra)

- **`BringWindowToTop` ativa o cartão** (o primeiro plano muda para o cartão). No host é igual: o hook reescreve
  `hwndInsertAfter`, mas não impede a ativação; o que protege o foco em uso normal é `MA_NOACTIVATE`. Isso foi
  tratado à parte, o foco foi devolvido e o resto da prova rodou com a referência refeita.
- **O redimensionamento da nota pelo laço modal ativa a nota**, como o host (notas ativam, são para digitar). A ordem
  Z foi corrigida pelo hook e a sombra continuou abaixo.

### Custo do acompanhamento (debug; 3 execuções, faixa)

| Evento | Tempo médio |
| --- | --- |
| mover: a sombra acompanha (`SetWindowPos`) | 0,34–0,44 ms por evento |
| redimensionar: imagem nova por nove fatias + `UpdateLayeredWindow` + `ResizeBuffers` do cartão | 7,4–7,9 ms por passo |
| reempilhar as sombras (`restack`, 7 janelas) | 1,6–2,1 ms por mudança de z |
| divergências entre a sombra e o cartão ao final de cada evento | **0** |

| Cartão (DIP) | Sombra completa (refaz o desfoque) | **Nove fatias** | Diferença máxima de alfa |
| --- | --- | --- | --- |
| 88 | 19,8–21,9 ms | **0,28–0,29 ms** | 1/255 |
| 188 | 63–72 ms | **0,93–1,13 ms** | 1/255 |
| 388 | 224–274 ms | **3,4–4,4 ms** | 1/255 |

As **nove fatias são de 70 a 75 vezes mais rápidas** que refazer o desfoque e ficam a 1/255 de alfa da sombra
completa (há teste unitário `nine_slice_matches_the_full_blur_within_two_alpha_levels`). **Sem elas, redimensionar
travaria** (200 a 270 ms por passo em um cartão de 388 DIP). O corte usado é a margem + 40 DIP.

## Defeitos da própria prova encontrados e corrigidos (não da técnica)

- O processo caía na saída (exceção `0x87A`, código 122/2170) porque as superfícies de DirectComposition eram
  destruídas depois do `Gfx`; agora são limpas antes. Saída 0 nas 3 execuções finais.
- Quando o navegador cobria os cartões, a entrada real só alcançava os cartões em modo de edição; esse caminho existe
  no código (passos 3, 4 e 6 viram TOPMOST) e chegou a rodar uma vez, mas com um erro de expectativa (passo 5 esperava o
  modo normal). Corrigido. **Nas 3 execuções finais os cartões estavam visíveis e a entrada real rodou em modo normal.**

## Limites (não verificado)

- **Sincronia visual cartão ↔ sombra:** a posição da sombra é atualizada de forma **síncrona** dentro do
  `WM_WINDOWPOSCHANGED` do cartão (0 divergências de retângulo), mas o cartão é composto pelo DirectComposition e a
  sombra por `UpdateLayeredWindow`: o DWM pode mostrar um quadro de diferença durante o arrasto. **Não foi medido nem
  observado a olho; precisa de captura em câmera lenta ou de teste manual.**
- **Notas ativam e o laço modal de redimensionamento** foi testado numa nota simplificada (um bitmap), não no editor
  Rich Edit real.
- **Janela de aplicativo real:** o "aplicativo" da prova é uma janela nossa; a interação com um navegador em tela cheia,
  `Win+D` (mostrar a área de trabalho), minimizar tudo e troca de área de trabalho virtual **não foi testada**.
- **S5 (conversão cartão ↔ janela em posição salva, encaixe na grade, limites de monitor, selos do modo de edição),
  S6 (UI Automation, alto contraste, "reduzir efeitos", DPI misto) não foram testados**; os cartões da prova só
  mostram um bitmap, sem widgets.
- Um monitor, 100% de escala, debug.
- O custo no DWM de 80 janelas continua sem medição conclusiva ([shadow-gate.md](shadow-gate.md)).

## Conclusão para o ADR-0005

1. **S2 passa no que foi exercitado:** com o mesmo hook de z do host, a sombra em camadas acompanha o cartão ao
   arrastar (captura de mouse), ao mudar de formato, ao redimensionar a nota pelo laço modal e no modo de edição,
   sem subir sobre aplicativos, sem a sombra alterar o foco e sem ficar acima de um cartão vizinho.
2. A implementação precisa de **três peças que não existem hoje no produto**: a janela de sombra por cartão com o mesmo hook;
   o acompanhamento em `WM_WINDOWPOSCHANGED`; e o `restack` das sombras a cada mudança de z. E, para o redimensionar
   não travar, **a sombra por nove fatias**.
3. **Ainda faltam para fechar S:** S5, S6, a sincronia visual cartão ↔ sombra, interação com aplicativos reais
   (tela cheia, `Win+D`), DWM em máquina quieta e em release, 150%/200%. Só depois disso a sombra ampla pode virar
   código de produto (outra etapa, com ADR aceito).

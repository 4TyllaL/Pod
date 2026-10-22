# Prova da sombra ampla (ADR-0005, etapa 2)

Data: 21 de outubro de 2026. Ambiente: Windows 11 build 26200, um monitor, escala do sistema 100%. Build de debug.
Código: `crates/pod-app/src/shadow_proof.rs`, rodado com `pod-app --shadow-proof <pasta>` (isolado: não cria cartões
do produto, não mexe em workspace nem na bandeja). Relatório bruto: [shadow-proof-report.md](shadow-proof-report.md);
imagem: [shadow-proof.png](shadow-proof.png) (dois cartões vizinhos a 12 DIP, fundo claro e escuro).

## O que foi comparado

Cartão de 88 DIP, raio 22, sombra preta de alfa 0,30 deslocada 8 DIP para baixo, margem de 24 DIP:

- **A — janela única com margem:** janela em camadas (alfa por pixel) de 136×136; `WM_NCHITTEST` devolve
  `HTTRANSPARENT` fora do cartão.
- **B — cartão + janela de sombra:** o cartão (88×88) e, logo atrás dele, uma janela só de sombra
  (`WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`).

Um **processo separado** (`--shadow-proof-target`) cria uma janela comum embaixo, que registra os cliques que
recebe. Em cada variação foram feitos **cliques reais** (`SendInput`, cursor restaurado depois) em quatro
pontos: centro do cartão, margem externa (alfa 0), margem com sombra à esquerda e abaixo.

## Resultados

| Critério | A (janela única) | B (cartão + janela de sombra) |
| --- | --- | --- |
| **S1 — clique** no centro do cartão chega ao cartão | sim | sim |
| S1 — clique na margem **externa** (alfa 0) passa para a janela de baixo | sim | sim |
| S1 — clique na margem **com sombra** passa para a janela de baixo | **NÃO** (2 de 2 pontos: a janela de prova ficou com o clique) | **sim** (2 de 2) |
| S2 — foco (primeiro plano não muda ao criar e clicar) | sim | sim |
| S3 — sombra do vizinho a 12 DIP sobre o cartão ao lado | alfa máx. 6/255 (2%), média 1,2/255 | igual (mesma sombra) |
| S4 — janelas para 40 cartões | 40 | **80** |
| S4 — pixels por cartão (vs. 7.744 hoje) | 18.496 (2,39×) | 26.240 (3,39×) |

**Achado principal:** a sombra com alfa maior que zero **intercepta cliques reais** de janelas de outro processo
quando o desenho é uma janela única com `HTTRANSPARENT` na margem. `WindowFromPoint` dizia o contrário (passava
pela janela), então ele **não serve** como prova de clique: só o teste com `SendInput` mostrou a falha. Isso é o
que o risco "região de clique" do ADR-0005 temia, e em um cartão de 88 DIP a margem de 24 DIP bloquearia, por
exemplo, ícones do desktop em volta do cartão. Só a variação B passou em todos os pontos.

Um erro da própria prova foi achado e corrigido no caminho (a janela A não estava acima do alvo por falta de
`WS_EX_TOPMOST` na criação); os números acima são da execução corrigida.

## Limites (não verificado)

- **Camada em vez de DirectComposition:** os cartões do Pod usam `WS_EX_NOREDIRECTIONBITMAP` + DirectComposition
  com swapchain pré-multiplicado. A prova usa janelas em camadas (`UpdateLayeredWindow`) porque a regra de clique
  por alfa é a mesma e é mais simples de isolar; **o caminho DirectComposition + margem não foi testado**.
- **Camada desktop não testada:** as janelas da prova foram `TOPMOST`; a combinação com `HWND_BOTTOM`,
  `MA_NOACTIVATE` e o hook de `WM_WINDOWPOSCHANGING` dos cartões (critério **S2** completo: arrastar,
  redimensionar, modo de edição) **não foi exercitada**.
- **Memória (S4) inconclusiva:** o delta de memória privada do processo ficou entre 4 e 20 KiB, ruído: bitmaps de
  janelas em camadas ficam no sistema e no DWM, não no processo. Vale só a contagem de janelas e de pixels (3,39×
  em B). **Custo de GPU/DWM e orçamento de 40 janelas continuam sem medição.**
- **S5 (geometria cartão ↔ janela), S6 (acessibilidade, DPI misto, alto contraste) não foram testados.**
- Uma máquina, um monitor, 100% de escala, sem papéis de parede variados atrás da sombra.
- Variação não testada: janela única com `SetWindowRgn` (cortaria também a sombra).

## Conclusão para o ADR-0005

- A recomendação original "S1: janela única com margem" **não passa** em S1 (clique) e deve ser trocada pela
  variação B (janela de sombra `WS_EX_TRANSPARENT` por cartão), que passou em clique e foco.
- B **dobra o número de janelas** (80 para 40 cartões) e exige manter a janela de sombra imediatamente atrás do
  cartão (z-order, posição, tamanho, visibilidade, foco do modo de edição). Esse custo precisa ser medido no
  portão de 40 janelas **antes** de decidir (S4 e S2 completos), junto com o caminho DirectComposition.
- Sem essa medição a sombra ampla **não entra**; o caminho conservador (`CS_DROPSHADOW`) segue válido.

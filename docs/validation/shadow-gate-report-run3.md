# Portao de 40 janelas: variacao B (ADR-0005, etapa 3)

### Cliques reais — janela de sombra em DirectComposition com `WS_EX_TRANSPARENT`

| Ponto | Quem recebeu o clique | Esperado | OK |
| --- | --- | --- | --- |
| centro do cartao | janela de prova | janela de prova | sim |
| margem externa (alfa 0) | janela de prova | janela de baixo | **NÃO** |
| margem com sombra (esquerda) | janela de prova | janela de baixo | **NÃO** |
| margem com sombra (abaixo) | janela de prova | janela de baixo | **NÃO** |

### Cliques reais — janela de sombra em DirectComposition com `WS_EX_LAYERED | WS_EX_TRANSPARENT`

| Ponto | Quem recebeu o clique | Esperado | OK |
| --- | --- | --- | --- |
| centro do cartao | janela de prova | janela de prova | sim |
| margem externa (alfa 0) | janela de baixo (outro processo) | janela de baixo | sim |
| margem com sombra (esquerda) | janela de baixo (outro processo) | janela de baixo | sim |
| margem com sombra (abaixo) | janela de baixo (outro processo) | janela de baixo | sim |

# Custo com 40 janelas

Escala do sistema 1.00; cartao 88 px, janela com margem 136 px; 40 cartoes em grade 10x4, todos visiveis (TOPMOST). Cada medida dura 6 s, com 2 s de acomodacao antes; "1 Hz" redesenha cada cartao/janela viva uma vez por segundo (como um relogio); janelas de sombra nunca redesenham.

| Fase | Janelas | Carga | CPU do Pod (% de 1 nucleo) | CPU do dwm (% de 1 nucleo) | GPU Pod (%) | GPU dwm (%) | WS Pod (MiB) | Privada Pod (MiB) | WS dwm (MiB) | PM dwm (MiB) | Mem. GPU dwm (MiB) | Mem. GPU Pod (MiB) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Sem janelas (linha de base) | 0 | ocioso | 0.00 | 5.00 | 0.00 | 0.36 | 39 | 28 | 498 | 614 | 589 | 10 |
| Sem janelas (linha de base) | 0 | 1 Hz | 0.25 | 1.67 | 0.00 | 0.39 | 39 | 28 | 498 | 614 | 589 | 10 |
| 40 cartoes, sem sombra (hoje) | 40 | ocioso | 0.12 | 6.67 | 0.00 | 0.36 | 41 | 30 | 499 | 615 | 590 | 11 |
| 40 cartoes, sem sombra (hoje) | 40 | 1 Hz | 2.26 | 8.50 | 0.77 | 0.47 | 41 | 31 | 499 | 615 | 592 | 12 |
| A: 40 janelas unicas com margem (DirectComposition; reprovada em clique) | 40 | ocioso | 0.11 | 7.50 | 0.00 | 0.39 | 41 | 31 | 499 | 615 | 599 | 20 |
| A: 40 janelas unicas com margem (DirectComposition; reprovada em clique) | 40 | 1 Hz | 3.38 | 6.83 | 1.08 | 0.55 | 41 | 31 | 499 | 615 | 609 | 30 |
| B: 40 cartoes + 40 janelas de sombra, ambas em DirectComposition | 80 | ocioso | 0.36 | 3.33 | 0.00 | 0.38 | 43 | 32 | 500 | 616 | 600 | 21 |
| B: 40 cartoes + 40 janelas de sombra, ambas em DirectComposition | 80 | 1 Hz | 2.67 | 5.83 | 0.87 | 0.48 | 43 | 32 | 500 | 616 | 602 | 22 |
| B2: 40 cartoes em DirectComposition + 40 sombras em camadas (UpdateLayeredWindow) | 80 | ocioso | 0.12 | 6.67 | 0.00 | 0.53 | 45 | 31 | 502 | 616 | 590 | 11 |
| B2: 40 cartoes em DirectComposition + 40 sombras em camadas (UpdateLayeredWindow) | 80 | 1 Hz | 2.24 | 17.33 | 0.86 | 0.70 | 45 | 31 | 503 | 641 | 617 | 12 |

`NaN` = o contador nao respondeu.

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
| Sem janelas (linha de base) | 0 | ocioso | 0.22 | 28.00 | 0.00 | 0.86 | 39 | 28 | 498 | 640 | 615 | 10 |
| Sem janelas (linha de base) | 0 | 1 Hz | 0.24 | 32.50 | 0.00 | 1.87 | 39 | 29 | 498 | 640 | 615 | 10 |
| 40 cartoes, sem sombra (hoje) | 40 | ocioso | 0.22 | 26.67 | 0.00 | 1.95 | 41 | 30 | 499 | 641 | 616 | 11 |
| 40 cartoes, sem sombra (hoje) | 40 | 1 Hz | 3.58 | 23.00 | 0.90 | 4.73 | 41 | 31 | 500 | 641 | 609 | 12 |
| A: 40 janelas unicas com margem (DirectComposition; reprovada em clique) | 40 | ocioso | 0.10 | 38.00 | 0.00 | 5.66 | 41 | 31 | 500 | 641 | 626 | 20 |
| A: 40 janelas unicas com margem (DirectComposition; reprovada em clique) | 40 | 1 Hz | 4.91 | 31.67 | 0.94 | 5.91 | 41 | 31 | 500 | 641 | 636 | 30 |
| B: 40 cartoes + 40 janelas de sombra, ambas em DirectComposition | 80 | ocioso | 0.20 | 26.17 | 0.00 | 5.52 | 43 | 33 | 501 | 642 | 628 | 21 |
| B: 40 cartoes + 40 janelas de sombra, ambas em DirectComposition | 80 | 1 Hz | 2.18 | 22.33 | 0.75 | 5.27 | 43 | 33 | 501 | 642 | 629 | 22 |
| B2: 40 cartoes em DirectComposition + 40 sombras em camadas (UpdateLayeredWindow) | 80 | ocioso | 0.12 | 24.00 | 0.00 | 5.50 | 45 | 32 | 504 | 642 | 618 | 11 |
| B2: 40 cartoes em DirectComposition + 40 sombras em camadas (UpdateLayeredWindow) | 80 | 1 Hz | 2.56 | 12.17 | 0.75 | 4.73 | 45 | 32 | 504 | 642 | 619 | 12 |

`NaN` = o contador nao respondeu.

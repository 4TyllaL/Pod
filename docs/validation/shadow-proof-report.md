# Prova da sombra ampla (ADR-0005, etapa 2)

Escala do sistema: 1.00 (card 88 px, margem 24 px, janela 136 px).

### A — janela única com margem e `HTTRANSPARENT` na margem

| Ponto | Sem janelas de prova | Com as janelas | Resultado esperado | OK |
| --- | --- | --- | --- | --- |
| centro do cartão | PodShadowTarget (pid 21328) | PodShadowProof (pid 8028) | cartão (nossa janela) | sim |
| margem externa (alfa 0) | PodShadowTarget (pid 21328) | PodShadowTarget (pid 21328) | a janela de baixo (cliques passam) | sim |
| margem com sombra (esquerda) | PodShadowTarget (pid 21328) | PodShadowTarget (pid 21328) | a janela de baixo (cliques passam) | sim |
| margem com sombra (abaixo) | PodShadowTarget (pid 21328) | PodShadowTarget (pid 21328) | a janela de baixo (cliques passam) | sim |

### Cliques reais — A

| Ponto | Quem recebeu o clique | Esperado | OK |
| --- | --- | --- | --- |
| centro do cartão | janela de prova | janela de prova | sim |
| margem externa (alfa 0) | janela de baixo (outro processo) | janela de baixo | sim |
| margem com sombra (esquerda) | janela de prova | janela de baixo | **NÃO** |
| margem com sombra (abaixo) | janela de prova | janela de baixo | **NÃO** |

### B — cartão + janela de sombra `WS_EX_TRANSPARENT` atrás

| Ponto | Sem janelas de prova | Com as janelas | Resultado esperado | OK |
| --- | --- | --- | --- | --- |
| centro do cartão | PodShadowTarget (pid 21328) | PodShadowProof (pid 8028) | cartão (nossa janela) | sim |
| margem externa (alfa 0) | PodShadowTarget (pid 21328) | PodShadowTarget (pid 21328) | a janela de baixo (cliques passam) | sim |
| margem com sombra (esquerda) | PodShadowTarget (pid 21328) | PodShadowTarget (pid 21328) | a janela de baixo (cliques passam) | sim |
| margem com sombra (abaixo) | PodShadowTarget (pid 21328) | PodShadowTarget (pid 21328) | a janela de baixo (cliques passam) | sim |

### Cliques reais — B

| Ponto | Quem recebeu o clique | Esperado | OK |
| --- | --- | --- | --- |
| centro do cartão | janela de prova | janela de prova | sim |
| margem externa (alfa 0) | janela de baixo (outro processo) | janela de baixo | sim |
| margem com sombra (esquerda) | janela de baixo (outro processo) | janela de baixo | sim |
| margem com sombra (abaixo) | janela de baixo (outro processo) | janela de baixo | sim |

### Foco (S2)

O primeiro plano não mudou após criar as janelas: A = sim, B = sim (esperado: sim).

### Memória e tempo com 40 janelas (S4)

| Configuração | Janelas | Pixels por cartão | Memória privada (KiB) | Tempo de criação (ms) |
| --- | --- | --- | --- | --- |
| Só cartão (hoje, sem margem) | 40 | 7744 | 0 | 240 |
| A — janela única com margem | 40 | 18496 | 4 | 242 |
| B — cartão + sombra | 80 | 26240 | 8 | 324 |

Razão de pixels A/hoje = 2.39×; B/hoje = 3.39×. Só a memória do processo (bitmaps de GDI e estruturas de janela); o custo no compositor (DWM/GPU) **não** foi medido.

### Vizinhança a 12 DIP (S3)

A margem de 24 DIP do cartão 2 invade o cartão 1 (intervalo de só 12 DIP). Alfa da sombra do cartão 2 sobre a faixa do cartão 1 sob essa margem: **máximo 6/255 (2%), média 1.2/255**. Um cartão branco coberto por alfa 6/255 de preto escurece até 249 em 255 nessa borda.

Imagem de revisão: `shadow-proof.png` (dois cartões vizinhos sobre fundo claro e escuro, o da direita por cima).

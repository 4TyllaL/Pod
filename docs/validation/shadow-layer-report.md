# Prova S2: sombra em camadas na camada desktop (ADR-0005, etapa 4)

Escala 1.00; cartão 88 px; 6 cartões de formato fixo (grade 3×2) + 1 nota (200×140 DIP); sombras em camadas com margem de 24 px.

### 1. Depois de criar (cartões no fundo, janela normal por cima; o primeiro plano é o de antes de criar qualquer janela)

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ABAIXO da janela normal | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |
| primeiro plano não mudou | sim |

_Primeiro plano ao fim deste passo: Chrome_WidgetWin_1 (pid 10388)._

### 2a. Tentativas de levantar cartão e sombra (HWND_TOP sem ativar)

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ABAIXO da janela normal | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |
| primeiro plano não mudou | sim |

_Primeiro plano ao fim deste passo: Chrome_WidgetWin_1 (pid 10388)._

### 2b. BringWindowToTop no cartão (ativa a janela; o hook só corrige a ordem)

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ABAIXO da janela normal | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |

_Primeiro plano ao fim deste passo: PodLayerCard (pid 11720)._

Informativo: `BringWindowToTop` mudou o primeiro plano (PodLayerCard (pid 11720)). Isso é o comportamento do host (o hook reescreve `hwndInsertAfter`, não bloqueia a ativação) e não depende da sombra; o foco foi devolvido para o resto da prova.

> Os cartões estavam cobertos por outra janela (Chrome_RenderWidgetHostHWND (pid 10388)); os passos 3, 4 e 6 abaixo rodam em **modo de edição (TOPMOST)** para a entrada real alcançá-los.

### 3. Clique real no cartão (SendInput)

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ACIMA da janela normal (modo de edição) | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |

_Primeiro plano ao fim deste passo: (nenhuma)._

Controle do foco depois de um clique real num cartão (primeiro plano antes: Chrome_WidgetWin_1 (pid 10388)): **com sombra** → (nenhuma); **sem sombra** (sombras escondidas) → (nenhuma). O resultado é o mesmo com e sem sombra: **não vem da sombra**; é o comportamento dos cartões com `MA_NOACTIVATE` nesta sessão.

### 4. Arrastar de verdade (SendInput, captura de mouse)

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ACIMA da janela normal (modo de edição) | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |
| (informativo) primeiro plano agora: (nenhuma) | sim |
| cartão moveu exatamente (140,90) px | sim |
| sombra acompanhou em cada passo (12 atualizações, 0 divergências) | sim |

_Primeiro plano ao fim deste passo: (nenhuma)._

### 5. Mudança de formato (SetWindowPos de tamanho)

| Verificação | OK |
| --- | --- |
| formato 188×88 DIP: cartão 188×88 px | sim |
| formato 188×188 DIP: cartão 188×188 px | sim |
| formato 88×88 DIP: cartão 88×88 px | sim |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ACIMA da janela normal (modo de edição) | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |
| (informativo) primeiro plano agora: (nenhuma) | sim |

_Primeiro plano ao fim deste passo: (nenhuma)._

### 6. Redimensionar a nota pelo canto (SendInput)

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ACIMA da janela normal (modo de edição) | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |
| nota cresceu pelo canto (laço modal do sistema) | sim |
| sombra acompanhou o tamanho em cada passo (14 imagens novas, 0 divergências) | sim |

_Primeiro plano ao fim deste passo: PodLayerCard (pid 11720)._

Informativo: o laço modal de redimensionamento do sistema mudou o primeiro plano (PodLayerCard (pid 11720)). Como no host, notas ativam (são para digitar) e o hook só corrige a ordem Z; cartões de formato fixo não redimensionam por borda. O foco foi devolvido para o resto da prova.

### 6b. Depois da entrada real: modo de edição desligado, tudo de volta ao fundo

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ABAIXO da janela normal | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |
| (informativo) primeiro plano agora: Chrome_WidgetWin_1 (pid 10388) | sim |

_Primeiro plano ao fim deste passo: Chrome_WidgetWin_1 (pid 10388)._

> **Achado (não é da sombra):** 1 cartão(ões) não ficaram `TOPMOST` com `SetWindowPos(HWND_TOPMOST)` (a chamada devolveu sucesso, o estilo não mudou) e só subiram depois de `HWND_NOTOPMOST` + `HWND_TOPMOST`. Aconteceu com a nota **depois do redimensionamento real pelo laço modal do sistema** (passo 6); sem esse redimensionamento todos subiram direto. Vale conferir `host::set_edit_layer` no produto.

### 7a. Modo de edição ligado (cartões TOPMOST, sombras logo abaixo)

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ACIMA da janela normal (modo de edição) | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |
| (informativo) primeiro plano agora: Chrome_WidgetWin_1 (pid 10388) | sim |

_Primeiro plano ao fim deste passo: Chrome_WidgetWin_1 (pid 10388)._

### 7b. Modo de edição desligado (tudo de volta ao fundo)

| Verificação | OK |
| --- | --- |
| todas as sombras abaixo de todos os cartões | sim |
| cartões e sombras ABAIXO da janela normal | sim |
| retângulo da sombra = cartão ± margem, em todos | sim |
| (informativo) primeiro plano agora: Chrome_WidgetWin_1 (pid 10388) | sim |

_Primeiro plano ao fim deste passo: Chrome_WidgetWin_1 (pid 10388)._

### Custo do acompanhamento (build de debug)

| Evento | Ocorrências | Tempo médio |
| --- | --- | --- |
| mover (sombra acompanha, `SetWindowPos`) | 19 | 239 µs |
| redimensionar (nova imagem por nove fatias + `UpdateLayeredWindow` + `ResizeBuffers` do cartão) | 17 | 6881 µs |
| reempilhar todas as sombras (`restack`, 7 janelas) | 51 | 1535 µs |
| divergências entre sombra e cartão | 0 | |

| Cartão (DIP) | Sombra completa (refaz o desfoque) | Nove fatias | Diferença máxima de alfa |
| --- | --- | --- | --- |
| 88 | 21083 µs | 273 µs | 1/255 |
| 188 | 63375 µs | 871 µs | 1/255 |
| 388 | 223229 µs | 2792 µs | 1/255 |

Cliques e arrastes reais: executados com `SendInput`.

**Resultado geral: todas as verificações passaram**

# Prova S5 e S6 da sombra ampla (ADR-0005, etapa 5)

Escala do sistema 1.00 (96 DPI); 3 monitor(es) nesta máquina:

| Monitor | Retângulo | Área útil | DPI | Principal |
| --- | --- | --- | --- | --- |
| LF24T35 | (1920, 0, 3840, 1080) | (1920, 0, 3840, 1080) | 96 | false |
| Odyssey G30B | (0, 0, 1920, 1080) | (0, 0, 1920, 1080) | 96 | true |
| LG FULL HD | (-1920, 0, 0, 1080) | (-1920, 0, 0, 1080) | 96 | false |

## S6.1 UI Automation

Cliente UI Automation (outra thread) conta, no processo, as janelas de **cartão** e de **sombra** em cada visão da árvore. Um leitor de tela navega pela visão de controle: a sombra não deveria aparecer lá.

| Provedor da janela de sombra | Cartões (bruta / controle) | Sombras (bruta / controle / conteúdo) | OK |
| --- | --- | --- | --- |
| padrão do HWND | 3 / 3 | 3 / 3 / 3 | (medição: sem mitigação) |
| provedor que não é de controle nem de conteúdo | 3 / 3 | 0 / 0 / 0 | sim |
| sem provedor (WM_GETOBJECT devolve 0) | 3 / 3 | 3 / 3 / 3 | não esconde (informativo) |

## Criação

| Verificação | OK |
| --- | --- |
| criar os cartões, as sombras e a janela normal não mudou o primeiro plano (início: Chrome_WidgetWin_1 (pid 10388); agora: Chrome_WidgetWin_1 (pid 10388)) | sim |

## S5.1 Geometria cartão ↔ janela (posicionamento e grade reais do produto)

Usa `pod_model::placement` (`to_preferred`, `resolve_all`) e `pod_model::grid` com layouts sintéticos (inclui coordenadas negativas e DPI misto). A posição gravada vem do retângulo do **cartão**, que o desenho da sombra em janela separada não altera; a sombra é cartão ± margem no DPI do monitor do cartão.

| Layout | Cartões | Dentro da área útil | Posição gravada ida e volta (±1 px) | Na grade | Sombras que passam da área útil | Sombras que invadem outro monitor (máx. px) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 monitor 1920×1080 a 96 DPI | 6 | 6/6 | 6/6 | 6/6 | 6 | 0 (0 px) |
| 2 monitores: principal a 96 DPI e um à esquerda 1920×1080 a 144 DPI | 12 | 12/12 | 12/12 | 12/12 | 11 | 3 (24 px) |
| 2 monitores: principal a 96 DPI e um à esquerda 2560×1440 a 192 DPI | 12 | 12/12 | 12/12 | 12/12 | 11 | 3 (24 px) |
| Monitor secundário removido (cartões remapeados para o principal) | 18 | 18/18 | n/a | n/a | n/a | n/a |

Resultado: todas as linhas passam (cartões dentro da área útil, posição gravada idêntica, na grade).

"Sombras que passam da área útil" e "invadem outro monitor" são informativos: um cartão na primeira coluna da grade fica a 12 DIP da borda e a sombra tem 24 DIP; no monitor vizinho aparece uma franja escura (mesmo cartão, outro monitor).

## S5.2 Bordas do monitor (esta máquina)

| Verificação | OK |
| --- | --- |
| cartão parcialmente fora à esquerda do desktop virtual: sombra = cartão ± 24 px | sim |
| cartão parcialmente fora embaixo e à direita: sombra = cartão ± 24 px | sim |
| cartão encostado no canto superior esquerdo: sombra = cartão ± 24 px | sim |
| cartão de volta à grade: sombra = cartão ± 24 px | sim |
| depois das bordas: todas as sombras abaixo de todos os cartões | sim |
| depois das bordas: cartões e sombras ABAIXO da janela normal | sim |
| depois das bordas: retângulo da sombra = cartão ± margem, em todos | sim |
| depois das bordas: primeiro plano não mudou | sim |

## S5.2b Cartão cruzando a borda entre monitores (monitores reais desta máquina)

| Verificação | OK |
| --- | --- |
| cartão metade em cada lado da borda x=1920 (DPI 96 | 96): sombra = cartão ± 24 px e inteira no desktop virtual | sim |
| cartão metade em cada lado da borda x=0 (DPI 96 | 96): sombra = cartão ± 24 px e inteira no desktop virtual | sim |
| depois de cruzar monitores: todas as sombras abaixo de todos os cartões | sim |
| depois de cruzar monitores: cartões e sombras ABAIXO da janela normal | sim |
| depois de cruzar monitores: retângulo da sombra = cartão ± margem, em todos | sim |
| depois de cruzar monitores: primeiro plano não mudou | sim |

## S5.3 Arrastar com encaixe na grade (SendInput)

| Verificação | OK |
| --- | --- |
> Cartões cobertos por outra janela; este passo roda em modo de edição (TOPMOST).

| o cartão se moveu | sim |
| posição final na grade do monitor (esquerda e topo) | sim |
| a sombra acompanhou cada posição encaixada (3 atualizações, 0 divergências, 0 posições fora da grade) | sim |
| depois do arraste: todas as sombras abaixo de todos os cartões | sim |
| depois do arraste: cartões e sombras ACIMA da janela normal (modo de edição) | sim |
| depois do arraste: retângulo da sombra = cartão ± margem, em todos | sim |
| depois do arraste: (informativo) primeiro plano agora: (nenhuma) | sim |

## S5.4 Selos do modo de edição (clique no selo, com a sombra do vizinho por baixo)

| Verificação | OK |
| --- | --- |
| cartão 1: no ponto do selo a janela é o cartão (não a sombra) | sim |
| cartão 1: clique real no selo chega ao cartão em (16, 16) do cliente | sim |
| cartão 2: no ponto do selo a janela é o cartão (não a sombra) | sim |
| cartão 2: clique real no selo chega ao cartão em (16, 16) do cliente | sim |
| nota: no ponto do selo a janela é o cartão (não a sombra) | sim |
| (informativo) clicar na nota ativou a nota (PodLayerCard (pid 12924)): como no host, notas ativam; o foco foi devolvido | |
| nota: clique real no selo chega ao cartão em (16, 16) do cliente | sim |
| a margem da sombra do cartão 2 realmente cobre a faixa direita do cartão 1 (cenário testado existe) | sim |
| nessa faixa a janela é o cartão 1, não a sombra do vizinho | sim |
| modo de edição: todas as sombras abaixo de todos os cartões | sim |
| modo de edição: cartões e sombras ACIMA da janela normal (modo de edição) | sim |
| modo de edição: retângulo da sombra = cartão ± margem, em todos | sim |
| modo de edição: (informativo) primeiro plano agora: Chrome_WidgetWin_1 (pid 10388) | sim |

## S6.2 Alto contraste e "reduzir efeitos"

Estado atual do sistema nesta máquina: alto contraste = false, animações desligadas = false, efeitos de transparência = true. Política: `sombra ampla = !alto_contraste && !animações_desligadas`.

Não alterei nenhuma configuração do sistema: as transições abaixo são **simuladas** com `POLICY_OVERRIDE` (a mesma função `apply_policy` que reage a `WM_SETTINGCHANGE`/`WM_THEMECHANGED`). A reação ao vivo ao alto contraste real fica para o modo `hold` (ver o fim).

| Verificação | OK |
| --- | --- |
| tabela da política (alto contraste ou sem animações → sem sombra; senão com sombra) | sim |
| política lida do sistema agora = !alto_contraste && !animações_desligadas | sim |
| política DESLIGA: nenhuma janela de sombra visível | sim |
| política DESLIGA: todos os cartões continuam visíveis | sim |
| sem sombra: mover e redimensionar o cartão continua funcionando | sim |
| política LIGA de novo: todas as sombras visíveis | sim |
| depois de religar: todas as sombras abaixo de todos os cartões | sim |
| depois de religar: cartões e sombras ABAIXO da janela normal | sim |
| depois de religar: retângulo da sombra = cartão ± margem, em todos | sim |
| depois de religar: (informativo) primeiro plano agora: Chrome_WidgetWin_1 (pid 10388) | sim |

## S6.3 DPI misto

Esta máquina tem 3 monitor(es) e DPI 96: **não há DPI misto real para testar**. O que segue é **simulação**: cartões com DPI diferentes ao mesmo tempo, e a troca de DPI de um cartão (o que `WM_DPICHANGED` faria ao cruzar monitores): a margem da sombra tem de acompanhar o DPI do cartão, sem fresta e sem imagem borrada.

| Verificação | OK |
| --- | --- |
| dois cartões a 96 e 192 DPI ao mesmo tempo: todas as sombras abaixo de todos os cartões | sim |
| dois cartões a 96 e 192 DPI ao mesmo tempo: cartões e sombras ABAIXO da janela normal | sim |
| dois cartões a 96 e 192 DPI ao mesmo tempo: retângulo da sombra = cartão ± margem, em todos | sim |
| dois cartões a 96 e 192 DPI ao mesmo tempo: (informativo) primeiro plano agora: Chrome_WidgetWin_1 (pid 10388) | sim |
| troca de DPI 96 → 120: cartão 110×110 px, margem 30 px, sombra = cartão ± margem, sem fresta | sim |
| troca de DPI 96 → 144: cartão 132×132 px, margem 36 px, sombra = cartão ± margem, sem fresta | sim |
| troca de DPI 96 → 168: cartão 154×154 px, margem 42 px, sombra = cartão ± margem, sem fresta | sim |
| troca de DPI 96 → 192: cartão 176×176 px, margem 48 px, sombra = cartão ± margem, sem fresta | sim |
| troca de DPI 96 → 240: cartão 220×220 px, margem 60 px, sombra = cartão ± margem, sem fresta | sim |
| depois do DPI misto: todas as sombras abaixo de todos os cartões | sim |
| depois do DPI misto: cartões e sombras ABAIXO da janela normal | sim |
| depois do DPI misto: retângulo da sombra = cartão ± margem, em todos | sim |
| depois do DPI misto: (informativo) primeiro plano agora: Chrome_WidgetWin_1 (pid 10388) | sim |

## Para você verificar ao vivo (alto contraste real)

```powershell
.\target\debug\pod-app.exe --shadow-s56 <pasta> hold
```

Isso abre 3 cartões com sombra por 2 minutos e escreve `live-log.md`: alterne o alto contraste (Alt esquerdo + Shift esquerdo + Print Screen) e as animações do Windows. Cada mudança de configuração que o Windows avisa registra uma linha com o estado lido e se a sombra ficou ligada ou desligada. **Não rodei esse modo**: ele depende de alterar configurações do sistema, que não faço.

**Resultado geral: todas as verificações passaram**

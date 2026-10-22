# ADR-0005 — Vidro como padrão e sombra ampla

**Status:** **proposta**; etapas 1 (medição V1), 2 (prova da sombra), 3 (portão de 40 janelas), 4 (S2: camada desktop) e 5 (S5/S6) executadas em 21/10/2026 — ver [glass-legibility.md](../validation/glass-legibility.md), [shadow-proof.md](../validation/shadow-proof.md), [shadow-gate.md](../validation/shadow-gate.md), [shadow-layer.md](../validation/shadow-layer.md) e [shadow-s56.md](../validation/shadow-s56.md). Nada de vidro padrão nem sombra implementado. **Data:** 21 de outubro de 2026.
**Origem:** o estudo "Pod – Mockup de widgets" mostra cartões translúcidos com sombra larga e suave. A entrega
de estilo macOS de 21/10 ([DESIGN.md §13](../DESIGN.md)) deixou essas duas partes de fora porque mexem em
contratos de plataforma. Este ADR decide **como** e **com que provas** elas podem entrar. Complementa o
[ADR-0002](0002-glass-backdrop.md) (vidro) e o [ADR-0004](0004-visual-direction.md) (direção visual: opaco
primeiro, vidro e sombra ampla como etapas próprias).

## 1. Contexto: o que existe hoje

- **Vidro simulado (ADR-0002, opção B):** implementado e **opt-in** (`preferences.glass`, item "Vidro
  (translúcido)" na bandeja). Não desfoca o fundo; opacidade 0,86 (claro) e 0,90 (escuro), escolhida para
  garantir 4,5:1 sobre fundo preto, branco ou cinza puros. Volta ao opaco com "efeitos de transparência"
  desligados ou em alto contraste. Padrão do app: **opaco**.
- **Vidro real (opção A, `DWMWA_SYSTEMBACKDROP_TYPE`):** nunca experimentado. Incertezas já listadas: recorte
  arredondado por janela e comportamento sem foco.
- **Sombra:** os cartões são janelas do **tamanho exato** do formato, recortadas por região arredondada.
  Qualquer pixel fora do cartão é cortado, então hoje só existe `CS_DROPSHADOW` (~3 px, só à direita e embaixo,
  some se o Windows desligar "sombras sob janelas"). A sombra larga do estudo (≈ 24 DIP de raio) não cabe nessa
  geometria (ADR-0001 D2).
- **Camada desktop:** cartões ficam em `HWND_BOTTOM`, sem ativar (`MA_NOACTIVATE`), com hook em
  `WM_WINDOWPOSCHANGING`. Orçamento de 40 janelas (D8) validado no portão de 40.

## 2. Decisão proposta

Tratar as duas partes **separadamente**, porque têm riscos e custos diferentes, e **manter o opaco como
padrão até as provas da seção 5 passarem**.

### 2.1 Vidro

1. **Não trocar o padrão agora.** Opaco continua o padrão; vidro simulado continua opt-in.
2. Promover o vidro simulado (B) a **padrão apenas quando** as provas V1–V4 abaixo passarem em hardware
   real, com papéis de parede reais. Mesmo então, ele continua desligável e cai para opaco nas condições de
   fallback já existentes.
3. Rodar o experimento A (backdrop do DWM) como **janela de prova isolada**, sem tocar nos cartões. Só vira
   candidata a padrão se passar V5. Não depende de B ser padrão.

### 2.2 Sombra ampla

Escolher entre três caminhos, **decidindo por medição**, não por preferência:

| Caminho | O que é | Prós | Contras / riscos |
| --- | --- | --- | --- |
| S0. Manter `CS_DROPSHADOW` | Status quo | Zero risco | Sombra curta, só em 2 lados, depende de ajuste do Windows |
| S1. Janela com margem (**reprovada na prova de 21/10**: a sombra intercepta cliques de outros processos; ver [shadow-proof.md](../validation/shadow-proof.md)) | Cada cartão vira uma janela maior (margem ≈ 24 DIP), em camadas com alfa por pixel; a sombra é desenhada em D2D dentro da margem; a região de clique continua sendo só o cartão | Sombra ampla e consistente; independe do ajuste do Windows | Mexe em geometria, grade, hit-test, a11y, camada desktop, memória (≈ 2,4× pixels no 1×1) |
| S2. Uma janela de sombra compartilhada | Uma janela de sombras atrás de todos os cartões | Cartões intactos | Quebra o modelo "um cartão = uma janela"; z-order e arrastar ficam complicados; **rejeitada** |

**Atualização após o portão de 40 janelas (etapa 3):** a janela de sombra precisa de `WS_EX_LAYERED | WS_EX_TRANSPARENT` (sem `WS_EX_LAYERED` bloqueia cliques até na margem de alfa 0, mesmo em DirectComposition); prefira a sombra em camadas (`UpdateLayeredWindow`), que não gasta memória de GPU do Pod (+≈10 MiB com swap chain); custo de CPU/GPU do Pod desprezível com 80 janelas; custo no DWM não resolvido (ruído maior que o efeito). S2 completo (`HWND_BOTTOM`/hook), S5, S6 e DWM em máquina quieta seguem pendentes. **Atualização após o S2 (etapa 4):** a sombra em camadas com o mesmo hook de z do host acompanhou o cartão ao arrastar (captura de mouse), mudar de formato, redimensionar a nota pelo laço modal e no modo de edição, sem subir sobre aplicativos, sem roubar foco e sem ficar acima de cartões vizinhos (3 execuções, 41 verificações cada, 0 falhas). Para redimensionar sem travar é necessária a **sombra por nove fatias** (70 a 75× mais rápida que refazer o desfoque, a 1/255 de alfa). Pendentes: S5, S6, sincronia visual cartão ↔ sombra no DWM, aplicativos reais (tela cheia, `Win+D`), DWM em máquina quieta/release, 150%/200%. **Atualização após S5/S6 (etapa 5):** S5 passa (posicionamento, grade, monitores e selos do produto continuam valendo; a sombra de um cartão perto da borda invade o monitor vizinho em até 24 px, a decidir). S6 passa **com uma exigência nova**: a janela de sombra precisa de um provedor de UI Automation que a esconda das visões de controle e de conteúdo (sem ele cada sombra é anunciada; devolver 0 em `WM_GETOBJECT` não resolve). Política: sem sombra em alto contraste nem com animações desligadas, funcionando simulada; **confirmação com alto contraste real pendente (modo `hold`, depende do usuário)**; **DPI misto só simulado** (os 3 monitores desta máquina são 96 DPI). Achados fora da sombra: clicar num cartão deixa "nenhuma janela" em primeiro plano (com e sem sombra; **confirmado no app real**, causa: falta de `WS_EX_NOACTIVATE`; **corrigido no host** em 21/10/2026, ver [focus-click.md](../validation/focus-click.md)) e a nota ignorou `HWND_TOPMOST` depois de redimensionar pelo laço modal. **Recomendação revisada após a prova (etapa 2):** trocar S1 por **S1b — cartão + janela de sombra `WS_EX_TRANSPARENT` atrás de cada cartão** (passou em clique e foco; custa o dobro de janelas). A frase original, abaixo, fica como histórico. **Recomendação original:** S1, mas só como **experimento isolado** (uma janela de prova e `--gallery`), com os critérios
S1–S6 abaixo. Se não passar, ficar em S0. S2 não será feita.

## 3. Riscos

**Vidro**
- *Legibilidade:* sem desfoque, mais transparência derruba o contraste sobre papéis de parede claros ou
  muito texturizados; por isso a opacidade atual é alta (0,86/0,90) e a diferença para o opaco é discreta.
  Baixar o alfa para chegar ao visual do estudo reabre esse problema.
- *Foco:* o acrílico do DWM pode "apagar" quando a janela perde o foco; o Pod nunca tem foco. Pode inutilizar A.
- *Recorte:* material do DWM é retangular por janela; o canto arredondado exige região ou preferência de canto
  e pode deixar halo.
- *Custo:* 40 janelas com backdrop no compositor podem estourar o orçamento de GPU.
- *Preferências do usuário:* transparência desligada no Windows precisa continuar levando ao opaco ao vivo.

**Sombra ampla (S1)**
- *Região de clique:* a margem transparente **não pode** capturar cliques nem bloquear o desktop (ADR-0004:
  "não aumentar a área invisível que intercepta cliques"). Exige hit-test por pixel/região só no cartão.
- *Sombras sobrepostas:* o intervalo da grade é 12 DIP e a sombra tem ≈ 24; sombras de vizinhos se sobrepõem e
  podem cobrir o cartão ao lado conforme a ordem de z.
- *Camada desktop:* janelas em camadas com alfa por pixel e `HWND_BOTTOM` + `MA_NOACTIVATE` nunca foram
  combinadas; arrastar e redimensionar podem piscar ou perder alfa.
- *Memória e custo de pintura:* a margem multiplica os pixels por janela (≈ 2,4× no 1×1, menos nos grandes);
  com 40 janelas e DPI alto isso precisa caber no orçamento de memória e manter zero quadros em repouso.
- *Geometria em vários lugares:* posicionamento, encaixe na grade, salvar e restaurar posição, modo de edição
  (selos nos cantos), `MoveWindow` e limites de monitor usam hoje o retângulo do cartão; todos precisam
  distinguir "retângulo do cartão" de "retângulo da janela".
- *Acessibilidade:* os limites que a UI Automation anuncia têm de continuar sendo os do cartão.
- *DPI misto:* a margem em DIP precisa arredondar igual em monitores de escalas diferentes, sem fresta.
- *Alto contraste e "reduzir efeitos":* sem sombra ampla nesses modos.

## 4. O que NÃO muda

Grade 88/12, formatos e resize livre de Notes, contratos de widget (view como dados), política de rede e
capabilities, e o fallback opaco idêntico ao de hoje (hashes da galeria inalterados nessas condições).

## 5. Critérios de validação (portões)

Cada critério precisa de **evidência registrada** em `docs/validation/` (comando, máquina, resultado). Teste de
regra ou galeria offscreen **não** substitui medição ao vivo.

**Vidro como padrão (V)**
- **V1 — legibilidade real:** texto ≥ 4,5:1 (principal ≥ 7:1) medido sobre ao menos 6 papéis de parede reais
  (claro, escuro, colorido, foto com muito detalhe, listrado e branco puro), nos dois temas, nos formatos 1×1,
  2×2 e 4×4.
- **V2 — fallback ao vivo:** ligar e desligar "efeitos de transparência" e alto contraste com o app aberto
  volta ao opaco e ao vidro sem reiniciar; no opaco, hashes da galeria idênticos aos de hoje.
- **V3 — sem regressão de custo:** portão de 40 janelas com vidro ligado dentro dos orçamentos de CPU e memória
  do M6/D8, zero quadros em repouso.
- **V4 — decisão de produto:** o dono confirma, vendo capturas em papéis de parede reais, que a diferença para
  o opaco justifica trocar o padrão.
- **V5 (só para A):** cantos corretos sem halo em 100/150/200% de DPI e DPI misto; cartão legível sem foco;
  GPU e memória com 40 janelas dentro do orçamento.

**Sombra ampla (S)**
- **S1 — clique:** com margem de 24 DIP, um clique na margem chega à janela/ícone **atrás** (verificado nos
  quatro lados de um cartão e entre cartões vizinhos); a região de input é exatamente o cartão.
- **S2 — camada desktop:** continua embaixo das outras janelas, não ativa, não rouba foco; arrastar, redimensionar
  e o modo de edição sem piscar nem perder alfa.
- **S3 — vizinhança:** com cartões a 12 DIP, a sombra de um não cobre o conteúdo do outro (inspeção em captura
  e teste de ordem de z).
- **S4 — orçamento:** portão de 40 janelas com margem: memória por janela e total dentro do orçamento em 100% e
  200% de DPI; zero quadros em repouso.
- **S5 — geometria:** posição salva e restaurada, encaixe na grade, limites de monitor e selos de edição
  continuam relativos ao **cartão**; teste de regra cobrindo conversão cartão ↔ janela.
- **S6 — acessibilidade e modos:** limites da UI Automation = cartão; sem sombra ampla em alto contraste e com
  "reduzir efeitos"; DPI misto sem fresta.

Se qualquer S1–S6 falhar sem correção razoável, a sombra ampla **não entra** e fica `CS_DROPSHADOW` (S0).

## 6. Ordem de trabalho (depois de aprovado)

1. Medir V1 com papéis de parede reais usando o vidro simulado que já existe (não exige código novo).
2. Janela de prova S1 isolada (camada + alfa por pixel + hit-test), sem tocar nos cartões.
3. Janela de prova do backdrop A (V5).
4. Só então decidir, com os números, o que vira padrão. Cada etapa em seu próprio commit e documento de
   validação.

## 7. Questões abertas (o dono decide)

1. Vidro padrão vale o risco se a diferença visual sobre o opaco for discreta (limite do vidro sem desfoque)?
2. Aceita investir no experimento A (desfoque de verdade) sabendo que pode ser inviável sem foco de janela?
3. Sombra ampla vale mexer na geometria das janelas, ou a sombra curta atual basta?
4. Em caso de dúvida, qual vence: fidelidade ao estudo ou o orçamento de 40 janelas?

## 8. Consequências

Nada muda no produto até a aprovação e as provas. Se aprovado, o trabalho começa por medição, não por código de
produção, e cada decisão de padrão fica atrelada a evidência. O caminho conservador (S0 + vidro opt-in) continua
válido e sem custo.

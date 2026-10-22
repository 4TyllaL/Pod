# ADR-0002 — Vidro e translucidez dos cartões (experimento de backdrop)

**Status:** **proposta** (nada implementado). **Data:** 18 de outubro de 2026.
**Origem:** ADR-0001 D10 e ARCHITECTURE §6 deixaram o vidro para depois do MVP e exigiram um ADR próprio.
O alvo visual é o das referências do dono: cartões arredondados translúcidos sobre o papel de parede.

## 1. Contexto

Hoje cada widget é uma janela com superfície **opaca** desenhada em D2D (`render::paint_tile_background`),
com `Plate`/papéis de cor separados do conteúdo. Blur de um bitmap D2D **não** borra o que está atrás da
janela; para isso só o DWM/compositor tem acesso ao desktop. Restrições já decididas: sem
`SetWindowCompositionAttribute` (não documentado), detecção de capacidade com fallback opaco, respeitar alto
contraste e "efeitos de transparência" desligados, orçamentos de D8 (40 janelas).

## 2. Opções

| Opção | O que é | Prós | Contras / riscos |
| --- | --- | --- | --- |
| A. Backdrop do sistema (`DWMWA_SYSTEMBACKDROP_TYPE`) | O DWM pinta Acrílico/Mica atrás da janela; o app desenha por cima com alfa | API documentada (build ≥ 22621); custo no compositor, não no app; respeita a configuração de transparência do sistema | Material é retangular por janela: o recorte arredondado do cartão precisa de região/`DWMWA_WINDOW_CORNER_PREFERENCE` ou de janela em camadas, **a verificar**; Mica/Acrílico dependem do estado de foco da janela (o acrílico "apaga" sem foco), **a medir** |
| B. Vidro simulado | Tinta translúcida + brilho/contorno sem desfocar o fundo | Funciona em qualquer versão, zero risco de compatibilidade, custo nulo | Não é vidro de verdade: sem desfoque do fundo; sobre papel de parede claro pode perder contraste |
| C. Captura do papel de parede + blur próprio | O app lê o wallpaper e desfoca no D2D | Controle total do visual | Não acompanha janelas atrás; custo de memória por cartão; privacidade e correção ruins; **rejeitada** |

## 3. Decisão proposta

1. Fazer **B primeiro**: material `Glass` no tema (papéis de cor com alfa, contorno e brilho sutil), ativado
   por configuração, com fallback opaco quando transparência estiver desligada ou em alto contraste. É o que
   cumpre D10 ("o tema comporta um material vidro") sem depender de comportamento do DWM.
2. Fazer **A como experimento isolado** em `--gallery`-like harness e uma janela de prova, com critérios de
   saída abaixo. Só vira padrão se passar.
3. **Não** fazer C.

## 4. Critérios de aceitação do experimento A

- Windows 11 build ≥ 22621 detectado por capacidade (não por número de versão fixo no código); em outra
  versão, ou com transparência desligada, o cartão fica opaco **idêntico ao de hoje** (hashes da galeria
  inalterados).
- Cantos arredondados corretos sem halo retangular, em 100/150/200% de DPI e em monitores de escalas
  diferentes.
- Texto legível sobre papel de parede claro, escuro e colorido, nos três temas; contraste WCAG do M6 mantido
  (o texto não pode depender de o fundo ser escuro).
- 40 janelas: CPU, memória e GPU dentro dos orçamentos do M6/D8; zero quadros em repouso preservado.
- Comportamento sem foco: o cartão não pode ficar ilegível quando a janela perde o foco (o Pod não rouba foco).
- Alto contraste e "efeitos de transparência" desligados voltam ao opaco ao vivo, sem reiniciar.

## 5. Questões abertas (o dono decide)

1. Aceita começar por B (vidro simulado) mesmo que não desfoque o fundo?
2. O cartão translúcido deve ser opt-in por widget, global ou por tema?
3. Prefere Acrílico (desfoque forte) ou Mica (mais discreto, tinge pelo papel de parede)? Só A permite.

## 6. Consequências

B é barato e reversível; A é o único caminho para "vidro de verdade" mas tem incertezas reais de janela
(recorte, foco) que só se resolvem medindo em hardware. Por isso o experimento A vem depois de B e depois de
fechar o portão de 40 janelas.

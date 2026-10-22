# W5 — Bolsa (quadro de cotações)

- Widget `pod.stocks` ("Bolsa (internet)", categoria Internet): quadro escuro com preço, ▲▼ e variação em
  verde (alta) e vermelho (baixa). Formatos 1×1, 2×1, 2×2, 2×3, 2×4, 4×1 e 4×2.
- **Três fontes, escolhidas no menu do cartão (grupo "Fonte"), e os ativos de cada uma (grupo "Ativos", até 8):**
  - AwesomeAPI: moedas e cripto em reais (sem chave). 8 ativos.
  - Brapi: ações da B3. **Sem token só atende PETR4, VALE3, ITUB4 e MGLU3** (verificado: os demais pedem token).
  - Yahoo Finance: B3 e mundo (endpoint público sem documentação oficial: pode mudar ou bloquear).
- Só com "Widgets de internet" ligado. Três hosts novos na lista permitida (11 no total). URLs montadas
  pelo host a partir de símbolos do catálogo; o widget nunca fornece URL. Atualiza a cada 90 s.
- Verificado: teste com internet real (`cargo test -p pod-app quotes -- --ignored`) leu as 3 fontes pelo
  corretor; teste ao vivo com 3 cartões (um por fonte) mostrou dados reais na tela.
- Preços com atraso, de serviços públicos; **não é recomendação de investimento**. Termos de uso de cada
  serviço a confirmar antes de distribuir o Pod. Sem token da Brapi (não foi feito o campo de token).

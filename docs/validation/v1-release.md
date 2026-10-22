# Pod 1.0.0 — notas de verificação da V1

Escopo pedido: uma V1 **sem** trabalho de Narrator/acessibilidade. Decisões: licença **MIT**, versão **1.0.0**.

## O que mudou para a V1

- Versão 1.0.0 e licença MIT (`Cargo.toml`, `LICENSE`, copiada para o pacote). O titular do copyright em `LICENSE` ("Pod authors") é um **placeholder**: troque pelo nome real.
- Ferramentas de prova (`--shadow-*`, `--focus-card`) só existem em **debug**; o executável de distribuição não contém essas strings (0 ocorrências; o debug tem 7).
- Primeira execução cria 3 widgets offline (Relógio, Calendário, Sistema) em vez de uma área vazia. Só em workspace novo: não se repete ao reabrir.
- Cartões de formato fixo usam `WS_EX_NOACTIVATE` (clicar não rouba o foco).
- README, README do pacote e privacidade reescritos conforme o comportamento real.

## Verificado (pacote `Pod-1.0.0-win-x64.zip`)

`tools/verify-package.ps1` (pastas e chaves de registro de teste; nada do seu perfil é tocado):

| Etapa | Resultado |
|---|---|
| Checksums de `SHA256SUMS.txt` | 0 divergências |
| Versão do `Pod.exe` | 1.0.0 / produto Pod |
| Instalar | exe, atalho e entrada em Aplicativos criados; sem início com o Windows |
| Primeira execução (dados novos) | 3 cartões visíveis, todos sem ativação, conteúdo lido por UI Automation; workspace gravado com 3 instâncias |
| Reabrir | continua 3 (sem duplicar) |
| Atualizar com o Pod aberto (`-CloseRunning`) | fechamento normal, reinício, 3 cartões |
| Desinstalar | exe, atalho e entrada removidos; dados preservados |

Também: testes automatizados passam no `tools/package.ps1` (310 no momento do empacotamento), build determinística (`--locked`, CRT estática, /Brepro, ZIP com datas fixas), verificações reais anteriores de foco, teclado nos botões, modo de edição e bandeja (`tools/verify-*.ps1`, [focus-click.md](focus-click.md)).

## NÃO verificado / fica fora da V1

- **Acessibilidade**: Narrator e outros leitores, foco de teclado nos cartões, IME. (Pedido explícito de ficar fora.)
- Windows 10, máquina virtual limpa, monitores com DPI diferentes.
- Assinatura digital (SmartScreen pode avisar); instalador é PowerShell, não MSI.
- Os scripts de verificação não foram reexecutados um a um contra o exe de release instalado; a verificação do pacote acima cobre ciclo de vida e primeira execução.
- 28 avisos do compilador (código usado só por testes e nomes de constantes do `windows-rs`); sem efeito funcional.
- Emoji restaurado de nota pode virar caixa; vidro sem desfoque; sombra ampla e vidro como padrão **não** entraram ([ADR-0005](../decisions/0005-glass-default-and-wide-shadow.md)).
- Chat de IA, cenas/mascote e atualização automática: fora do escopo.
- Nada foi commitado (a pasta não é repositório git).

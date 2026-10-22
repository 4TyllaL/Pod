# W3 e vidro: teste ao vivo e portão de 40 janelas

Ambiente: Windows 11 build 26200, **um monitor 2000×1200 a 144 DPI (150%)** (a memória antiga falava em 3
monitores a 100%: a máquina mudou), 12 núcleos lógicos, GPU real. Build release com asserts de debug
(`--target-dir target-perf`), dados em `--data-dir` temporário. Scripts de apoio ficaram fora do repositório.

## Teste ao vivo (janelas reais)

| Item | Resultado |
|---|---|
| Rede | Janela 282×132 px (188×88 DIP a 150%). UIA: `Rede`, `↓ 60 KB/s`, `↑ 369 KB/s`, `Download nos últimos minutos, agora 60 KB/s`. O gráfico apareceu e correu. |
| Downloads | Pasta vazia → UIA `vazia`. Criei `pod-teste-ao-vivo.txt` → em ≤ 13 s: `1 arquivo`, `pod-teste….txt`, `0 KB · agora`. Apaguei → voltou a `vazia`. |
| Capturas | `423 arquivos`, duas linhas com nome abreviado (`Screenshot ….png`), tamanho e idade (`18 KB · há 30 min`). |
| Área de transferência | Texto copiado apareceu com espaços e quebra de linha colapsados. O conteúdo original era um texto; foi restaurado ao final (o preview voltou ao mesmo). |
| Vidro (menu) | O menu da bandeja abriu pela mensagem real do ícone; **Up, Up, Enter** chegou em "Vidro (translúcido)". Com fundo magenta atrás de um cartão de CPU, o pixel livre foi de `R28 G31 B37` (opaco) para `R51 G27 B58` (translúcido, magenta transparecendo). Captura visual confirmou o texto legível. |
| Persistência | `preferences.glass` virou `true` em ≤ 5 s; ao reiniciar o Pod o cartão já abriu translúcido (mesmo pixel); alternar de novo gravou `false`. |

Limites do teste ao vivo: os itens do menu nativo não aparecem na árvore UIA (não consegui ler os nomes), então
o item foi achado pela posição no menu; o estado "marcado" do item não foi lido. Janelas de teste se
sobrepõem (minha geração de posições), então só um cartão por cenário serve para amostrar pixel. Foco/teclado do
menu dependeram do menu estar em primeiro plano.

## Portão de 40 janelas (D8) — 60 s em repouso, após 8 s de aquecimento

| Cenário | CPU da máquina | Memória privada | Handles | Quadros | p95 de UI | Leituras |
|---|---|---|---|---|---|---|
| 40 cartões novos (10 Rede, 10 Downloads, 10 Capturas, 10 Área de transf.) | 0,022% (0,156 s) | 38,2 → 38,4 MB | 1289 → 1293 | 320 (≈ 5,3/s = 10 Redes a cada 2 s) | 0,39 ms (máx 0,57) | rede 33 (2 s); Downloads/Capturas 7 cada; clipboard 1 |
| Mesmo cenário **com vidro ligado** | 0,026% | 38,3 → 38,5 MB | 1287 → 1291 | 320 | 0,52 ms | igual |
| Misto (10 CPU 1×1, 10 Memória 2×1, 10 Rede 2×2, 10 Clipboard 1×1) | 0,020% | 38,5 → 38,5 MB | 1287 → 1291 | 680 | 0,41 ms | CPU 34, memória 33, rede 33 |

Orçamentos do M6 (CPU < 0,1%, p95 < 8 ms, sem crescimento de memória) **atendidos nos três cenários**, e o vidro
não mediu custo adicional. O `total_p95` ≈ 29 ms é o tempo até o `Present(1)` sincronizar com o vblank, não
trabalho de UI. Handles variaram +4 em 60 s (já variavam assim no W1); não rodei 3 minutos para provar ausência de
vazamento como no W1.

## Defeito achado e corrigido

Com um cartão de Área de transferência ligado (intervalo de 1 s), **Downloads e Capturas eram relidos a cada
segundo** (66 leituras em 60 s) em vez de a cada 10 s. Agora cada pasta tem cache de 10 s (`FOLDER_TTL`); o
cache some sem assinante. Remedido: 7 leituras em 60 s. Teste novo
`folders_are_cached_between_ticks_and_dropped_without_subscribers`.

## Não verificado

- GPU/VRAM dos 40 cartões (informativo no ADR; não medi neste ambiente).
- Vidro sobre papel de parede real, 3 temas ao vivo e alto contraste/"efeitos de transparência" desligados no
  Windows (não alterei configurações do sistema; o fallback só tem teste de regra).
- Área de transferência com conteúdo marcado como privado ao vivo (só o provedor e o widget em teste).
- Música, DPI misto e vários monitores (esta máquina tem um só).
- Suítes: 194 no `pod-app` e 105 no `pod-model` passam.

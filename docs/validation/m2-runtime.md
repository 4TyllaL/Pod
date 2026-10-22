# M2 — Runtime e independência de instâncias

Ambiente: Windows 11, build de debug, 100% DPI, um monitor lógico de teste.

## O que existe

- `pod-model`: `WidgetId`/`WidgetTypeId`, contrato `Widget` (`handle`, `view`, `snapshot`, `stop`),
  `Event`, `Effect`, `Outcome`, `Element` (superfície, rótulo, botão, campo), `Phase` com transições.
- `pod-app/runtime`: registro de tipos, instâncias com geração, lifecycle
  (`Loaded → Mounted → Running → Stopping → Disposed`), visibilidade como flag, agendador único de
  prazos, invalidação por instância, `remove` idempotente.
- `pod-app/shell`: dono do runtime; mapa instância→HWND; um `SetTimer` na janela de controle oculta
  (sem prazo pendente, sem timer).
- `pod-app/widgets/test_card`: widget fictício (contador + relógio de 1 s). O host não tem lógica de
  Notes; é o mesmo host que servirá Quick Notes.
- `pod-app/ui`: layout vertical puro da view e hit testing.

## Testes automatizados (23 em `pod-app`, 6 em `pod-model`)

Invariantes cobertos: IDs nunca duplicados (inclusive com fonte que colide); evento de uma
instância não altera outra; evento antes de `Running` é recusado; factory que falha (tipo
desconhecido, estado inválido, versão nova) não deixa instância; remoção cancela prazos; prazo de
geração antiga não atinge instância nova; `remove` idempotente; ocultar não muda a fase nem o
estado; suspensão de 1 h gera um único disparo; 2 000 ciclos criar/remover sem resíduo; layout sem
sobreposição nem tamanhos negativos.

## Verificação em Windows real (capturas e automação de mouse/teclado)

| Cenário | Resultado |
| --- | --- |
| Duas instâncias renderizam rótulos, botão e contador de segundos | OK; os segundos avançam via agendador central |
| 3 cliques no botão de uma instância | contador 0→3 só nela; 3× `StateChanged` com o ID dela |
| Ctrl+Shift+N / Ctrl+Shift+W | janelas `PodWidget`: 2→3→2 (contadas via `EnumWindows`) |
| 150 ciclos criar+remover (após 10 de aquecimento) | handles 1067→1063, GDI 7→7, USER 23→23, working set 42→42 MB |

Medido com `GetGuiResources`, `Process.HandleCount` e `WorkingSet64`, em build de debug.

## Limitações e pendências

- Sem bandeja: atalhos de teste (Ctrl+Shift+N/W/H/S) substituem comandos até o M4. Ocultar uma nota
  sem bandeja a torna irrecuperável, exceto por Ctrl+Shift+S a partir de outra janela visível.
- O editor Rich Edit ainda existe (`text.rs`), mas só é criado para widgets com campo de texto; o
  `test_card` não tem, então a prova do M1 não roda neste binário. Será religado no M4. Por isso há
  avisos de código não usado (editor, `snapshot`, `ids`), esperados até M3/M4.
- `StateChanged` só é registrado; snapshot, ack e remoção só após gravação são do M3.
- A visibilidade foi verificada nos testes de regra, não por captura de tela.
- Não testado: monitores/DPI mistos, device loss, Explorer reiniciando.
- Medições são de debug, uma máquina, sem repetição; não são metas de desempenho calibradas.
- IME real, Narrator e emoji definido por código seguem pendentes do M1.

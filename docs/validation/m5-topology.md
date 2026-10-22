# M5 — Topologia e recuperação de plataforma

Ambiente: Windows 11, build de debug. Três monitores reais de 100% (origem negativa no da esquerda).
Testes com `POD_DATA_DIR` (só debug); o perfil real não foi tocado.

## O que existe

- `pod-model/placement` (puro): `PreferredPlacement` (offsets e tamanho em DIPs relativos à área útil
  de um monitor, razões e área de referência, `MonitorHint`), `Monitor`, `match_monitor` (caminho exato →
  EDID inequívoco, desempatado só por geometria única → topologia só sem identidade → principal; ambíguo
  nunca vira correspondência), `effective_rect` (offsets se a área útil é a mesma, razões se mudou, sempre
  dentro da área útil, piso de emergência 80×60 DIP), `resolve_all` (remapeadas para o principal em
  cascata por ordem estável de ID), `locate` (posição absoluta do schema 1 → relativa ao monitor).
- Schema 2: placement preferido relativo ao monitor; migração do schema 1 em cópia. O arquivo antigo é
  preservado em `recovery/premigration-v1-*` antes de qualquer gravação.
- `monitors.rs`: `EnumDisplayMonitors` + `GetDpiForMonitor` + `QueryDisplayConfig`/
  `DisplayConfigGetDeviceInfo` correlacionados pelo nome GDI (`monitorDevicePath`, EDID, nome amigável).
- Shell: preferido gravado (nunca o efetivo); `apply_topology` recalcula e reposiciona; só um movimento do
  usuário (fim do loop modal com a janela realmente em outro lugar) confirma nova preferência;
  `WM_DISPLAYCHANGE`/área útil/`WM_DPICHANGED` com debounce de 400 ms.
- `WM_POWERBROADCAST` e `WM_WTSSESSION_CHANGE`: suspender/bloquear envia pendências e para o agendador;
  retornar revalida topologia, bandeja e desenho.
- Perda de dispositivo: o desenho que falha agenda a reconstrução (nunca dentro do `WM_PAINT`); recria
  dispositivo e superfícies com o mesmo estado; WARP a partir da 3ª tentativa consecutiva; para após 6.
- Pausa por tela cheia: hook de primeiro plano + debounce; esconde temporariamente as notas do monitor
  coberto (limites de moldura do DWM, tolerância 2 px, excluindo Pod e shell), sem tocar na visibilidade
  gravada; reavalia a cada 1 s enquanto ativa.

## Testes automatizados (57 em `pod-app`, 31 em `pod-model`)

Placement (17): caminho exato; EDID único com caminho novo; monitores idênticos desempatados só por
geometria única; ambiguidade sem correspondência; topologia só sem identidade; remapeamento em ordem
estável de ID com cascata independente da ordem de entrada; reconexão restaura sem deriva; área útil
nova usa razões e fica dentro; offsets exatos quando a área é a mesma; área minúscula e degenerada;
ida e volta 200× em 200% com origem negativa sem deriva; tamanho em DIPs preservado entre escalas;
monitor por sobreposição e por proximidade; schema 1 resolvido por topologia ou remapeado; números
inválidos. Schema/storage: migração v1→v2 com original byte a byte preservado antes de gravar.

## Verificação em Windows real (automação de mouse/teclado)

| Cenário | Resultado |
| --- | --- |
| Enumeração real | 3 monitores com nome, `monitorDevicePath`, EDID (`2D4C:707D`, `2D4C:7305`, `6D1E:5B55`), geometria e DPI corretos |
| Abrir schema 1 com notas nos 3 monitores | posições idênticas às gravadas; v1 preservado em `recovery/` (hash igual); arquivo ainda v1 até haver mudança |
| Primeira edição | grava schema 2 com monitor e offsets relativos; `legacy_abs` some |
| Desconexão **simulada** dos secundários | notas da esquerda e da direita vão para o principal em cascata (0,0 e 24,24), dentro da área útil; preferido no arquivo inalterado |
| Reconexão simulada | posições voltam exatamente às originais |
| Arrastar nota remapeada | nova preferência no monitor principal (100,60) e persiste após reconectar e após matar/reabrir |
| Clique simples na faixa de uma nota remapeada | preferência não é sobrescrita |
| 5× `WM_DISPLAYCHANGE` em rajada | uma única reavaliação (debounce); `WM_SETTINGCHANGE` de área útil dispara, outros tipos não |
| Suspender/bloquear (mensagens postadas) | texto digitado 60 ms antes está em disco em 250 ms (antes do debounce de 500 ms); o controle sem suspender ainda não estava |
| Perda de dispositivo (simulada) | 1 rodada: recria com adaptador padrão; 3 rodadas: tentativa 3 usa WARP; texto e edição intactos, desenho normal |
| Janela sem borda cobrindo o monitor principal | só a nota daquele monitor some; as outras ficam; reaparece ao fechar |
| Janela do tamanho da área útil | não esconde nenhuma nota |

## Limitações e pendências (não fingir cobertura)

- **Nada disso foi testado com hardware mudando.** Desconexão é simulada por um atalho de debug
  (Ctrl+Shift+R) que remove os secundários da lista; reconexão idem. Os handlers de mensagem foram
  exercitados com mensagens postadas/enviadas, não com um monitor real, mudança de resolução, rotação,
  dock ou RDP. As três telas estão a 100%: **DPI misto só foi coberto por testes de regra**, nunca ao vivo.
- Monitores idênticos e remapeamento ambíguo: só testes de regra.
- Suspensão e bloqueio são mensagens simuladas, não um sleep real; "stop timers" só tem efeito sobre o
  agendador (as notas não têm timers).
- Perda de dispositivo é um erro injetado (`DXGI_ERROR_DEVICE_REMOVED`), não um reset real de GPU/driver;
  WARP só provou que o dispositivo é criado e desenha. Sem indicação visual de "modo degradado" (só log).
  Sem teste com GPU híbrida.
- Tela cheia: testada com uma janela sem borda de outro processo, não com jogo ou vídeo. Não usa
  `SHQueryUserNotificationState` (previsto na arquitetura). Uma janela maximizada com barra de tarefas
  oculta é indistinguível de tela cheia pela geometria e esconderia as notas. Não há reavaliação por
  mudança de tamanho do primeiro plano além do polling de 1 s enquanto ativa.
- Reinício do Explorer e a bandeja: o tratamento de `TaskbarCreated` existe, mas a bandeja segue sem
  verificação funcional (ver M4).
- Desktops virtuais e Win+D: nenhum comportamento próprio nem teste.
- Os testes ao vivo mostraram rodadas instáveis quando o foco do teclado estava em outra janela (atalhos
  perdidos); a rodada final passou. A instabilidade é do harness (foco), não foi investigada no app.
- Atalhos de teste Ctrl+Shift+R/L/K existem só em builds de debug.

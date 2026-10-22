Pod @VERSION@ - widgets para o desktop do Windows
==================================================

O Pod coloca widgets (cartões) na sua área de trabalho, atrás dos seus aplicativos: relógio, calendário,
lembretes, timers, métricas do sistema, música, clima e outros, mais notas de texto. Tudo volta como estava
depois de reiniciar o Windows ou o Pod.

Requisitos
- Windows 11, 64 bits (x64): é onde o Pod foi testado. Windows 10 22H2 deve funcionar, mas NÃO foi testado.
- Não precisa de administrador, de conta, de serviço do Windows nem de instalar runtimes.

Instalar (para o seu usuário)
1. Extraia esta pasta em qualquer lugar.
2. Abra o PowerShell na pasta extraída e rode:
       powershell -ExecutionPolicy Bypass -File .\install.ps1 -Launch
   Isso copia o Pod para %LOCALAPPDATA%\Programs\Pod, cria o atalho no Menu Iniciar e registra o Pod em
   Configurações > Aplicativos. O Pod NÃO inicia com o Windows, a menos que você peça: -Autostart.

Primeira vez
- Na primeira execução aparecem 3 widgets: Relógio, Calendário e Sistema. Eles ficam na área de trabalho,
  atrás das janelas (use "Mostrar a área de trabalho" para vê-los).
- O ícone do Pod fica na bandeja (pode estar em "Mostrar ícones ocultos"). O menu dele tem: Nova nota, Novo widget, Editar widgets, Meus widgets e notas (para recuperar),
  Ocultar todos e Ajustes (Iniciar com o Windows, Modo desempenho, Vidro e Widgets de internet), e Sair.
- Modo desempenho (ligado por padrão): quando um aplicativo cobre o monitor inteiro (por exemplo um jogo
  em tela cheia sem borda), os cartões daquele monitor somem e voltam quando você sai. É uma heurística
  de tamanho de janela, não identifica jogos; jogos em tela cheia exclusiva não foram testados.

Usar
- Arrastar: clique e arraste o cartão; ele encaixa numa grade.
- Tamanho: puxe a borda do cartão (os formatos disponíveis dependem do widget).
- Clique direito num cartão: ocultar, editar widgets, mover pelo teclado, excluir e os ajustes do widget.
- Editar widgets: abre a galeria na base da tela. Clique num widget (ou arraste) para adicionar; "-" no canto do cartão
  remove; um espaço "+" tracejado escolhe onde o próximo widget nasce; "Restaurar padrão" volta aos 3 widgets
  iniciais depois de confirmar (as notas não são apagadas). Esc ou Concluído sai da edição.
- Notas: texto simples. Menu da nota: botão "..." na faixa de cima, clique direito na faixa, Shift+F10 ou
  tecla de menu. Alt+F7 move e Alt+F8 redimensiona com as setas (Shift = passo maior, Enter confirma, Esc
  cancela). Fechar a nota (Alt+F4) a oculta; ela volta pela bandeja. Excluir pede confirmação se há texto.

Usar sem instalar (portátil)
   Pod.exe --data-dir "D:\MeusDadosDoPod"
   Sem --data-dir, os dados vão para %LOCALAPPDATA%\Pod.

Atualizar
1. Extraia a versão nova.
2. Rode o install.ps1 dela com -CloseRunning. Ele pede ao Pod, de forma normal, que feche (os dados são
   gravados), troca o executável e inicia de novo. Nunca encerra o Pod à força. Seus dados não são tocados.
   Se o arquivo de dados for de uma versão mais nova que o Pod instalado, o Pod se recusa a abrir e a
   gravar (para não danificar nada): instale a versão correta.

Desinstalar
   powershell -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\Programs\Pod\uninstall.ps1"
   (ou Configurações > Aplicativos > Pod). Seus dados são PRESERVADOS em %LOCALAPPDATA%\Pod.
   Para apagá-los também: acrescente -RemoveData (irreversível).

Privacidade
- Nada sai do seu computador por padrão. Não há telemetria, conta nem atualização automática.
- Widgets de internet (Clima, Notícias, GitHub, Bolsa) ficam DESLIGADOS até você ligar "Widgets de internet"
  no menu da bandeja. Ligados, o Pod só fala com endereços https de uma lista fechada, sem seguir
  redirecionamentos. O que é enviado: o nome da cidade (Clima), o pedido de cada feed ou cotação e, no
  GitHub, o token que VOCÊ informar (guardado no Gerenciador de Credenciais do Windows, nunca em arquivo).
- Alguns widgets leem dados locais, só se você os adicionar: Uso do Claude lê os registros de uso em
  ~/.claude/projects; Música lê a sessão de mídia do Windows; Notificações pede acesso às notificações do
  Windows; Downloads e Capturas de tela listam essas pastas; Área de transferência lê a área de transferência.
- Seus dados ficam em %LOCALAPPDATA%\Pod\workspace.json, em texto simples (sem criptografia). Cópias de segurança e
  arquivos preservados numa recuperação ficam em %LOCALAPPDATA%\Pod\recovery. O texto das notas não vai para logs.

Assinatura
- Este pacote @SIGNSTATE@. O SmartScreen do Windows pode avisar sobre um aplicativo desconhecido.
  Confira o hash em SHA256SUMS.txt.

Limitações conhecidas desta versão
- ACESSIBILIDADE: leitores de tela (Narrator) e teclado nos cartões não foram validados. Os botões dos cartões
  respondem ao UI Automation (Invoke), mas não há como dar o foco do teclado a um cartão. O método de entrada
  (IME, por exemplo japonês) não foi testado. Esta versão não deve ser considerada acessível.
- Emoji e símbolos salvos em uma nota podem aparecer como caixas depois de reabrir (o texto está intacto;
  digitado de novo, aparece).
- Vidro (translúcido) é opcional e não desfoca o fundo. A sombra dos cartões é curta (do Windows).
- Testado só em Windows 11, nesta máquina de desenvolvimento: não em Windows 10, nem em máquina virtual limpa,
  nem com monitores de escalas diferentes.
- O pacote não é assinado. O instalador é um script PowerShell, não um MSI.

Licença: MIT (arquivo LICENSE). Licenças de terceiros: THIRD-PARTY-LICENSES.txt.

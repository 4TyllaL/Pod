# Vidro simulado (ADR-0002, opção B) — opt-in

Ambiente: Windows 11 build 26200, build de debug.

## O que existe

- `Theme.surface_alpha` (1,0 por padrão) e `Theme::glass()`: superfície do cartão translúcida **sem desfocar
  o fundo**. Opacidade 0,86 (claro) e 0,90 (escuro). Não faz nada no alto contraste e é idempotente.
- Para o texto continuar legível sem saber o que há atrás do cartão, o modo vidro reforça as cores: texto
  principal e secundário mais fortes, acentos do tema claro escurecidos e cores semânticas
  (positivo/aviso/perigo) trocadas por variantes de mais contraste. Só valem enquanto o vidro está ligado.
- `render::paint_tile_background` usa o alfa nas duas pontas do gradiente. O contorno do vidro é um pouco
  mais forte (0,22). Só cartões (`Chrome::Tile`) mudam: Quick Notes e o editor Rich Edit continuam opacos.
- Opt-in: item **"Vidro (translúcido)"** no menu da bandeja (marcado quando ligado), gravado em
  `preferences.glass` do workspace (sem mudar o schema: vai em `Preferences.extra`), padrão desligado.
- Fallback: `sysinfo::transparency_enabled()` lê `EnableTransparency`; com "efeitos de transparência"
  desligados no Windows, ou em alto contraste, o cartão fica opaco **idêntico ao de hoje**. A mudança do
  Windows chega pela mesma mensagem de tema (`ImmersiveColorSet`) que já reavalia o tema.
- Galeria: temas `glass-light` e `glass-dark` (mais 196 hashes, 400 no total). Os hashes opacos anteriores
  **não mudaram**.

## Testes

- `glass_is_readable_on_any_backdrop`: texto ≥ 7:1 e papéis secundário/acento/semânticos ≥ 4,5:1 sobre fundo
  totalmente preto, branco e cinza médio, nos dois temas, nas duas pontas do gradiente.
- `glass_never_applies_to_high_contrast_or_twice`: alto contraste inalterado, idempotência, padrão opaco.
- Regressão visual dos 5 temas; a checagem de "cartão quase vazio" passou a contar alfa ≥ 200.
- Suítes: 193 no `pod-app`, 105 no `pod-model`, todas passando.

## Limites e o que não foi verificado

- **Não é vidro de verdade**: o fundo não é desfocado. Com 0,86/0,90 de opacidade a translucidez é discreta;
  garantir 4,5:1 sobre qualquer papel de parede exigiu esse valor. Baixar a opacidade piora a legibilidade
  nos extremos (preto e branco puros).
- **Não testei ao vivo** o item de menu, a gravação/leitura da preferência nem a troca de "efeitos de
  transparência" do Windows. A galeria foi renderizada offscreen e olhei duas imagens (CPU escuro, Relógio claro).
- Ver o efeito sobre papéis de parede reais não foi feito (as imagens da galeria têm fundo transparente).
- A opção A (backdrop do DWM) do ADR-0002 segue como experimento futuro.

## Polimento: cor de identidade por widget

Cada tipo de cartão tem agora a sua cor (`widgets::accent_for`: azul, vermelho-alaranjado, verde ou roxo, por
exemplo Timer/Pomodoro/Calendário quentes, Bateria/Disco/Downloads verdes). O fundo do cartão recebe uma
lavagem dessa cor, forte no topo e quase sumida na base (`header_tint_alpha × 0,45`; não existe no alto
contraste). Notas continuam com a cor por ordem de criação. Todos os hashes da galeria foram regravados
(mudança intencional); a revisão foi visual (Timer claro e Bateria escura) e pelos testes de contraste, que
seguem passando. Não verificado ao vivo.

## Polimento: sombra e espaçamento

- **Sombra:** os cartões são janelas do tamanho exato do formato, recortadas por região arredondada; o que se
  desenha fora do cartão seria cortado, então uma sombra própria em D2D não é possível sem mudar a geometria
  (ADR-0001 D2). Em vez disso a classe `PodWidget` ganhou `CS_DROPSHADOW`. Testado ao vivo com fundo magenta:
  aparece uma sombra curta (~3 px) nas bordas direita e inferior, acompanhando os cantos arredondados. É
  discreta, não é a sombra larga das referências, vale também para as notas, e some se o Windows tiver
  "sombras sob janelas" desligadas. Sombra larga exigiria janelas com margem (decisão de arquitetura).
- **Espaçamento:** no 1×1 dos medidores (CPU, Memória, Disco, Bateria, Progresso) o espaço entre o anel e o
  rótulo foi de 2 para 5 DIP. Hashes regravados; conferido na galeria (CPU 1×1 escuro).
- Tipografia: os pesos já estavam definidos por estilo (Título semi-negrito, numerais leves); não mexi.

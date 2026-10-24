# Forma Desktop — guia para quem chega (e para o Claude dele)

Este arquivo é o ponto de partida de quem vai trabalhar no `forma-desktop` a partir da
v0.1.0. Ele foi escrito para ser lido pelo **Claude Code**: copie-o (ou renomeie) para
`CLAUDE.md` na pasta onde você clonar os repositórios, para o Claude carregá-lo sozinho em
cada sessão. Leia também o `README.md`, o `../libforma/README.md`, o `ROADMAP.md` e o
`../libforma/docs/format.md`.

## O projeto em uma frase

Forma é uma ferramenta de design gráfico **livre (GPL-3.0-or-later) e sem fins lucrativos**,
que une modelos prontos com edição vetorial: canvas infinito, quadros (pranchetas) com
tamanhos prontos, formas, caneta, texto, camadas, booleanas e exportação PNG/SVG/PDF. Tudo
local, sem conta e sem internet.

## Quem faz o quê

| Pessoa | Responsabilidade |
|---|---|
| **denkalli** (dono do projeto) | `libforma` (núcleo), decisões de produto, revisões e lançamentos. |
| **Você** | `forma-desktop`: compilar e empacotar as versões **Windows e macOS para a v0.2.0**, e fazer os **polimentos** que levam à **v1.0.0**. |

**Você não altera a `libforma`.** Se precisar de algo nela (uma função nova, uma correção,
uma mudança de API), descreva o que precisa e por quê numa issue em
<https://github.com/denkalli/libforma/issues>, ou converse com o denkalli. Para o Claude:
**nunca edite arquivos em `../libforma`**. Se uma tarefa parecer exigir isso, pare e explique
o que seria necessário.

## Repositórios e montagem local

- <https://github.com/denkalli/libforma>: núcleo (modelo, formato `.forma`, renderização,
  exportação). Tag `v0.1.0`.
- <https://github.com/denkalli/forma-desktop>: o app (binário `forma`). Tag e release
  `v0.1.0` (só Linux, Flatpak).

O desktop depende da `libforma` **por caminho** (`libforma = { path = "../libforma" }`), então
os dois precisam ficar lado a lado:

```
forma/                  ← abra o Claude Code aqui (e ponha este arquivo aqui como CLAUDE.md)
  libforma/             ← só leitura para você
  forma-desktop/        ← onde você trabalha
```

```sh
git clone https://github.com/denkalli/libforma
git clone https://github.com/denkalli/forma-desktop
```

Para reproduzir exatamente a v0.1.0, faça `git checkout v0.1.0` nos dois.

## Ferramentas

- **Rust ≥ 1.89**, edição 2024. O Vello 0.11 exige 1.89; use o stable mais recente.
- GPU com Vulkan, Metal ou Direct3D 12. O canvas e a exportação PNG usam o wgpu, e os testes
  de GPU e de texto são pulados quando não há placa de vídeo ou fontes.
- **Linux:** para o Flatpak, `flatpak` e o `org.flatpak.Builder`. O README do desktop diz quais
  runtimes instalar.
- **Windows:** toolchain MSVC (Visual Studio Build Tools).
- **macOS:** Xcode Command Line Tools. Para binário universal, os alvos
  `aarch64-apple-darwin` e `x86_64-apple-darwin`.

## Verificação (rode antes de todo commit)

```sh
cd forma-desktop
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --release perf -- --ignored --nocapture   # tempos do editor numa página com 5 500 camadas
```

Estado na v0.1.0: 66 testes passando, mais 1 de desempenho ignorado, e clippy limpo. Nenhum
commit deve quebrar isso.

### Ver a interface sem capturar a área de trabalho

O app grava uma captura da própria janela e fecha. O Claude deve usar isso e ler o PNG com a
ferramenta Read para conferir mudanças visuais:

```sh
FORMA_SCREENSHOT=/tmp/x.png forma <arquivo.forma | pasta>
```

| Variável | Efeito |
|---|---|
| `FORMA_SCREENSHOT_SELECT=<nome da camada>` | Seleciona uma camada antes da captura. |
| `FORMA_SCREENSHOT_OPEN=new\|export\|files\|fonts\|settings\|settings-project\|templates` | Abre um diálogo, uma lista ou uma página. `fonts` precisa de um texto selecionado. |
| `FORMA_THEME=light\|dark` e `FORMA_LANG=en\|pt` | Escolhem tema e idioma sem mexer nos ajustes. |

Os ajustes ficam em `dirs::config_dir()/forma/settings.json`. Para não tocar nos seus, use
uma pasta temporária: no Linux, `XDG_CONFIG_HOME=/tmp/cfg`; em outros sistemas, verifique onde
o `dirs` grava. Um arquivo de teste pronto é o
`../libforma/tests/data/showcase-v1.forma` (formato v1, que migra sozinho).

## Stack (decidida — não reabrir sem falar com o denkalli)

- **egui/eframe 0.37**, sem `system_fonts`. Essa feature puxa um fontique antigo que quebra o
  Parley; a interface usa a fonte Inter embutida em `assets/fonts`.
- **wgpu 30**: precisa ser a mesma versão do Vello (via `libforma`). Atualizar egui ou wgpu
  significa atualizar a `libforma` junto. Não faça isso sozinho.
- **Ícones:** `lucide-icons`, desenhados com uma família de fonte própria (`theme/icons.rs`).
- **Diálogos de arquivo:** `rfd`, nativo em cada sistema. **Pastas do sistema:** `dirs`.
- A crate `winit` é dependência direta só para manter as decorações de janela no Wayland.
- Sem `egui_dock`, que ainda está no egui 0.36.

## Arquitetura do desktop

```
src/
  main.rs          janela (app id "io.github.denkalli.Forma"), argumento de linha de comando
  app.rs           FormaApp: ações (enum Action), arquivos, pastas, layout, atalhos, diálogos
  settings.rs      Settings (JSON); migra campos antigos em from_json
  workspace.rs     pastas de projetos e de modelos (template_dirs)
  i18n/            tr/trf/tr_noop; pt.rs = tabela portuguesa
  editor/          estado do editor, SEM tipos do egui
    mod.rs         Editor, Tool, Viewport
    interaction.rs eventos de ponteiro, arrastos, snapping, regras de seleção em quadros
    ops.rs         comandos (alinhar, distribuir, agrupar, colar, páginas…)
    geometry.rs    redimensionar/girar no espaço da página
    export.rs      ExportTarget e exportação
    tests.rs       testes do editor (dirigidos pelos mesmos eventos do canvas)
  canvas/          canvas do egui: textura do Vello, grade de pontos, réguas, overlays, menu
  ui/              painéis e diálogos (inspector, layers, pages, home, toolbar, settings,
                   font_picker, widgets)
  theme/           tokens.rs (cores e tamanhos) e egui.rs (o único que conhece o egui)
```

### Regras que o Claude precisa seguir

- **Toda mudança no documento passa por `libforma::history::Command`** (`editor.apply`). Arrastos
  e campos contínuos usam `apply_merged` com uma chave, para virar um único passo de desfazer.
- **O estado do editor não usa tipos do egui.** Toda lógica nova de edição vai para
  `editor/` e ganha teste em `editor/tests.rs`; a interface só chama métodos do `Editor`.
- **Tema:** cores e tamanhos só em `theme/tokens.rs`. Só `theme/egui.rs` conhece o egui.
- **Textura do canvas:** `Rgba8Unorm`, exibida no egui **sem** view sRGB (com sRGB, a gama seria
  aplicada duas vezes). A view de exibição tem `usage: TEXTURE_BINDING`. O Vello renderiza com
  fundo transparente; o egui desenha antes o fundo e a grade.
- **Idiomas:**
  - Todo texto visível é escrito **em inglês** no código, como `tr("New design")`. Textos com
    valores usam `trf("{n} layers", &[("n", &n.to_string())])`, e textos guardados em
    constantes usam `tr_noop("…")`, passando por `tr` na hora de mostrar.
  - Cada texto novo precisa de tradução em `src/i18n/pt.rs`, na ordem alfabética do inglês.
    O teste `every_text_is_translated` aponta o que falta, o que sobra e marcadores diferentes.
  - O inglês é o padrão; o usuário escolhe o idioma em Ajustes.
- **Layout do inspetor:** use `widgets::field_row::<N>` e os campos que ocupam a coluna. Um
  widget que pede um pouco mais que a largura disponível faz o painel crescer sozinho até o
  máximo, num ciclo de realimentação do egui. Meça a largura exata, como em `color_field`.
- **Marca:** nunca mencione Canva ou Figma em READMEs, metadados ou textos do app. A inspiração
  é só de layout e fluxo; nomes, cores e marca são do Forma.
- **Idioma do trabalho:** código, comentários, README e `docs/` em inglês. A conversa com você
  segue a língua que você usar.
- **Mudanças grandes:** planeje antes (plan mode) e combine com o denkalli se mexerem em
  produto ou visual.

## Sua missão para a v0.2.0: Windows e macOS

A v0.1.0 foi testada só no Linux (CachyOS, KDE Wayland, NVIDIA). O código evita dependências
exclusivas de Linux, mas há pontos conhecidos que precisam de trabalho.

### Pontos específicos de Linux no código

1. **`app.rs::open_in_file_manager`** chama `xdg-open`. No Windows use `explorer`, no macOS
   `open`, com `#[cfg(target_os = …)]` ou uma crate como `opener` (combine a dependência com o
   denkalli).
2. **`workspace.rs::template_dirs`** procura modelos em `<exe>/../share/forma/templates`, o
   layout do Linux e do Flatpak. No macOS deve ser `Forma.app/Contents/Resources/templates`;
   no Windows, uma pasta `templates` ao lado do `.exe`, ou a do instalador.
3. **Atalhos:** o código usa `Modifiers::COMMAND`, que já vira ⌘ no macOS. Mas os rótulos nos
   menus e dicas dizem "Ctrl" (por exemplo `tr("Save   Ctrl S")`). No macOS eles devem mostrar
   ⌘; mude a formatação, sem duplicar as traduções.
4. **Fontes:**
   - O texto dos documentos usa as fontes do sistema via fontique: DirectWrite no Windows,
     CoreText no macOS.
   - A família genérica `sans-serif` muda de fonte entre sistemas, e já muda entre o Flatpak
     e o binário nativo.
   - As fontes adicionadas pelo usuário ficam em `dirs::data_dir()/forma/fonts`.
5. **GPU:**
   - O Vello precisa de textura de armazenamento `Rgba8Unorm`.
   - Confira no Metal (macOS) e no DX12/Vulkan (Windows) se o canvas e a exportação PNG
     funcionam, inclusive com escala de tela 2× (Retina).
6. **`main.rs`:** o app id do Wayland não vale nos outros sistemas, mas o título da janela e o
   ícone precisam ser configurados para eles. O egui aceita um ícone no `ViewportBuilder`.
7. **Associação de arquivos `.forma`:** no Linux vem do MIME do Flatpak. No macOS é preciso
   declarar um UTI no `Info.plist`, e no Windows registrar a extensão pelo instalador.

### Empacotamento

- **Windows:**
  - um `.zip` portátil, mais um instalador (MSI com `cargo-wix`, ou Inno Setup);
  - ícone `.ico` gerado de `packaging/flatpak/io.github.denkalli.Forma.svg`;
  - assinatura de código é opcional (sem ela, o SmartScreen avisa).
- **macOS:**
  - `Forma.app` (por exemplo com `cargo-bundle`) e `.dmg`, binário universal (`lipo` com
    aarch64 e x86_64);
  - `Info.plist` com bundle id `io.github.denkalli.Forma`, UTI e ícone `.icns`;
  - sem conta de desenvolvedor da Apple o app não é notarizado: documente o "abrir mesmo
    assim".
- Ponha o empacotamento em `packaging/windows/` e `packaging/macos/`, como já existe
  `packaging/flatpak/`, cada um com um script de build e as instruções no README.

### CI (o ROADMAP ainda tem isso em aberto)

GitHub Actions em `forma-desktop/.github/workflows/`, com uma matriz `ubuntu`, `windows` e
`macos`:
- clona a `libforma` ao lado (na tag combinada, ou na `main`);
- roda fmt, clippy e testes;
- gera os pacotes como artefatos.

Os testes de GPU são pulados sozinhos onde não houver placa de vídeo.

### Lançamento da 0.2.0

1. Suba `version` no `Cargo.toml` e acrescente um `<release>` no metainfo.
2. Rode a verificação nos três sistemas.
3. Combine com o denkalli a tag da `libforma` usada.
4. Crie a tag `v0.2.0` no `forma-desktop` e uma release no GitHub com os pacotes Linux
   (`.flatpak`), Windows e macOS.

## Polimentos rumo à 1.0.0 (lista inicial)

- **Menu de contexto:** o clique direito no canvas e o "Adicionar fontes…" foram testados só
  pela lógica do editor. Confira com o mouse, nos três sistemas.
- **Seletor de fontes:**
  - a prévia não funciona para famílias genéricas (`sans-serif`);
  - a lista corta quando a janela é baixa.
- **Edição de texto no canvas:** o campo de edição no lugar não respeita a altura fixa nem o
  alinhamento vertical da caixa.
- **Atalhos com ⌘** e mensagens de erro amigáveis por sistema.
- **Desempenho no Windows e no macOS:** rode o teste ignorado de desempenho e compare com o
  Linux. Lá, um passo de arrasto leva cerca de 1,5 ms numa página com 5 500 camadas.
- **Acessibilidade e teclado:** foco, navegação por Tab e leitores de tela (o eframe tem
  `accesskit`).
- **Tela de modelos:** os modelos oficiais entram depois da 1.0.0, mas a página já lê `.forma`
  da pasta de modelos.
- Reveja `ROADMAP.md` (Fases 2 e 3) com o denkalli antes de escolher o que entra na 1.0.0.

## Fluxo de trabalho no git

- Trabalhe em branches (`win/…`, `mac/…`, `fix/…`) e abra pull requests para a `main` do
  `denkalli/forma-desktop`.
- Mensagens de commit em inglês, no imperativo ("Add Windows installer").
- Não faça push direto na `main`, nem crie tags ou releases sem combinar.
- O Claude não deve fazer commit, push, tag ou release sem você pedir.

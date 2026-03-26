# Artemis

High-performance terminal workstation for real-time C-to-Assembly mirroring.

## Architecture

**Core**: Rust TUI (ratatui + crossterm)  
**Watcher**: Async file monitor (notify)  
**Pipeline**: GCC compilation on file change  
**Mirror**: .loc directive parsing for C↔ASM synchronization

## Installation

Requires Rust toolchain. Install from [rustup.rs](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Build

Using Makefile:

```bash
make build          # Debug build
make release        # Optimized release build
```

Or with cargo directly:

```bash
cargo build --release
```

## Usage

Using Makefile:

```bash
make run            # Builds and runs with example.c
```

Or with cargo directly:

```bash
./target/release/artemis program.c
```

## Make Targets

- `make build` - Build in debug mode
- `make release` - Build optimized release
- `make run` - Build and run with example.c
- `make test` - Run tests
- `make check` - Check without building
- `make fmt` - Format code
- `make clippy` - Run linter
- `make asm` - Generate assembly from example.c
- `make clean` - Clean build artifacts
- `make help` - Show all targets

## Controls

- `q`: Quit
- `↑/↓`: Scroll C source
- `PgUp/PgDn`: Scroll assembly

## Interactive Editing (tui-textarea)

- C source pane is now an editable text area (`tui-textarea`) with arrow keys, insert/delete, and cursor movement.
- Typed input is debounced (300ms) and automatically triggers `gcc -S` recompilation of `example.c`.
- The assembly pane highlights lines mapped to the current C cursor position via `.loc` directives.
- Status bar shows mode, binary and RSP telemetry with inverted high-contrast style.

## C-to-Assembly Mapping Logic

The synchronization mechanism relies on GCC's DWARF debug symbols embedded in the assembly output when compiled with `-g`:

### .loc Directive Structure

```asm
.loc <file_id> <line_number> <column>
```

Example:
```asm
.loc 1 5 0
movl $10, -4(%rbp)
.loc 1 6 0
movl -4(%rbp), %eax
```

### Mapping Algorithm

1. **Parse Phase**: Iterate through `.s` file line-by-line
2. **Extract**: When `.loc 1 N 0` is found, record: `C_line[N] → ASM_line[current_index]`
3. **Store**: Build `HashMap<usize, Vec<usize>>` where key = C line, value = ASM line indices
4. **Lookup**: Given C cursor position at line `N`, query map for corresponding ASM block

### Edge Cases

- Multiple ASM instructions can map to single C line (loop unrolling, inlining)
- Compiler optimizations may reorder or eliminate instructions
- `-O0` and `-fno-stack-protector` flags preserve 1:1 correspondence

### Implementation

See `compiler.rs::parse_loc_directives()` for full parser logic.

## GCC Flags

```
-S                    Generate assembly
-masm=intel          Intel syntax
-fno-stack-protector Disable canary insertion
-g                   Emit debug symbols
-O0                  No optimization
```

## Visual Theme (Vantablack Palette)

Artemis now uses a custom high-contrast terminal theme for a cyberpunk hardware feel.

### Palette constants (Rust)

```rust
const VANTABLACK: Color = Color::Rgb(0, 0, 0);       // Pure Black
const NEON_GREEN: Color = Color::Rgb(0, 255, 65);    // Primary Text
const DIM_GREEN: Color = Color::Rgb(0, 100, 25);     // Inactive/Borders
const CYBER_CYAN: Color = Color::Rgb(0, 255, 255);   // Keywords/Registers
const ALERT_RED: Color = Color::Rgb(255, 0, 50);     // Segfaults/Errors
```

### Pane style rules

- `C` editor and `ASM` view: background `VANTABLACK`.
- `SUBTITLE`/title text: `CYBER_CYAN` + `Modifier::BOLD`.
- Borders: thick or rounded with `DIM_GREEN` color.
- C editor text: `NEON_GREEN`.
- ASM hex/instruction text: AI choreography where `mov/push/pop/add/sub` is `CYBER_CYAN`, directives (`.` prefix) are `DarkGray`, everything else is `NEON_GREEN`.

### Footer pulse bar style

- Base line: top border `DIM_GREEN`.
- Status chunk: inverted style (`NEON_GREEN` background, `VANTABLACK` foreground).
- RSP field: `CYBER_CYAN`.
- Error statuses: `ALERT_RED`.

### Example layout code

```rust
let c_pane = Paragraph::new(c_code)
    .style(Style::default().bg(VANTABLACK).fg(NEON_GREEN))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(DIM_GREEN))
            .title(Span::styled(" SOURCE [C] ", Style::default().fg(CYBER_CYAN).add_modifier(Modifier::BOLD)))
    );

let asm_pane = Paragraph::new(asm_lines)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM_GREEN))
            .title(Span::styled(" ASSEMBLY [ASM] ", Style::default().fg(CYBER_CYAN).add_modifier(Modifier::BOLD)))
    );

let pulse_bar = Paragraph::new(pulse_line)
    .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(DIM_GREEN)));
```

> Ensure your project imports the required Ratatui types:
> `use ratatui::style::{Color, Modifier, Style};`
> `use ratatui::widgets::{Block, Borders, BorderType, Paragraph, Wrap};`
> `use ratatui::text::{Line, Span};`


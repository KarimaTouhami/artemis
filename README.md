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

## Color Scheme

- Background: `#000000` (Vantablack)
- Borders: `#333333` (Dark Gray)
- Highlights: `#00FF41` (Neon Green)

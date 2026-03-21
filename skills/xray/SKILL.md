---
name: xray
description: Explore codebases efficiently using xray's layered, budget-aware queries. Use when navigating unfamiliar repos, finding functions/classes/structs, or orienting in a codebase before making changes.
user-invocable: true
allowed-tools: [Bash, Read, Glob, Grep]
argument-hint: "[skeleton|outline] [path] [--kind source|test|config|ci|docs|build] [--lang rust|python|typescript] [--pattern glob] [--budget N]"
---

# xray

Explore codebases using `xray` - a CLI tool that provides layered, budget-aware queries designed for efficient codebase navigation. Instead of reading entire files or running `tree`, use xray to get structured overviews that minimize context consumption.

## Prerequisites

`xray` must be installed (`~/.cargo/bin/xray`). If not found, install it:

```bash
cargo install --path ~/repos/scottidler/xray --locked
```

## Exploration Strategy

Always follow this progressive drill-down pattern. Start broad, then narrow.

### Step 0: Size the codebase with tokei

Before exploring, get a quick sense of scale:

```bash
tokei <path>
```

This tells you lines of code per language in one call. Use it to calibrate your approach:

| Codebase size | Approach |
|---------------|----------|
| Small (< 5k lines) | `outline --kind source` will fit comfortably, no budget needed |
| Medium (5k-20k lines) | Scope with `--lang`, `--kind`, or `--pattern` |
| Large (> 20k lines) | Always scope with `--pattern` to specific directories; use `--public` to cut volume |

### Step 1: Orient with skeleton

Get the project layout. This is cheap (typically 20-40 lines).

```bash
xray skeleton
```

This shows:
- Detected languages (rust, python, typescript)
- Directory tree with noise collapsed (node_modules, __pycache__, etc. hidden; data/, examples/ summarized)
- File kind annotations (source, config, test, ci, docs, build)

### Step 2: Outline the area of interest

Once you know which directory or language matters, get symbol signatures. Use the tokei output and skeleton structure to choose appropriate filters:

```bash
# Outline a specific directory
xray outline --pattern "src/auth/**"

# Outline only source files in a language
xray outline --lang rust --kind source

# Large repo - public API only for a specific area
xray outline --pattern "src/api/**" --public
```

This shows function/class/struct/trait/interface signatures with line numbers - enough to know what exists and where, without reading full files.

### Step 3: Read specific files

Now you know exactly which file and line to read. Use the Read tool directly on the specific location xray pointed you to.

## Command Reference

### Layers

| Layer | Purpose | Typical cost |
|-------|---------|-------------|
| `skeleton` | Directory tree with smart collapsing | 20-40 lines |
| `outline` | Symbol signatures with line numbers | varies by scope |

### Filters (combinable)

| Flag | Description | Example |
|------|-------------|---------|
| `--kind` / `-k` | Filter by file kind (repeatable) | `-k source -k test` |
| `--lang` / `-l` | Filter by language (repeatable) | `-l rust -l python` |
| `--pattern` | Scope to glob (repeatable) | `--pattern "src/api/**"` |
| `--exclude` | Exclude glob (repeatable) | `--exclude "*.generated.*"` |
| `--budget` / `-b` | Max output lines (0 = unlimited) | `-b 500` |
| `--public` | Public symbols only (outline) | `--public` |
| `--private` | Private symbols only (outline) | `--private` |
| `--format` / `-f` | Output format: json, yaml, auto | `-f json` |

### File kinds

`source`, `test`, `config`, `ci`, `docs`, `build`

### Supported languages

`rust`, `python`, `typescript` (includes JavaScript)

## Usage Patterns

### Unfamiliar repo - full orientation

```bash
tokei .                                # How big is this?
xray skeleton                          # What's the structure?
xray outline --kind source             # What are the main symbols?
```

### Find where something lives

```bash
xray outline --pattern "**/*auth*"     # Find auth-related symbols
xray outline --pattern "src/api/**"    # What's in the API layer?
```

### Multi-language repo

```bash
xray skeleton                          # See all detected languages
xray outline --lang python --kind source  # Just Python source symbols
xray outline --lang rust --kind source    # Just Rust source symbols
```

### Scoped investigation

```bash
xray outline --pattern "src/models/**" --public  # Public API of models
xray skeleton --kind test                         # Where are the tests?
xray outline --kind config                        # What config files exist?
```

### Large codebase (> 20k lines)

```bash
tokei .                                           # Confirm scale
xray skeleton                                     # Get directory layout
xray outline --pattern "crate_name/src/**" --public  # One crate at a time
xray outline --pattern "src/specific_module/**"   # Drill into a module
```

### Budget as a safety net

The default budget is unlimited. Only use `--budget` as a backstop to prevent accidentally dumping thousands of lines. Prefer scoping with filters first.

```bash
xray outline --kind source --budget 500  # Safety cap, not primary control
```

If budget is exceeded, xray exits with code 1 and prints the overage to stderr. Narrow scope with `--kind`, `--pattern`, or `--public` rather than just increasing the budget.

## Output Format

- **TTY (interactive):** YAML output for readability
- **Piped:** JSON output for machine consumption
- **Override:** `--format json` or `--format yaml`

Every response includes a `lines:` footer showing the line count consumed.

## Rules

1. **Run `tokei` first** on unfamiliar repos to calibrate your filter strategy
2. **Always start with `skeleton`** before jumping to `outline` - orient first
3. **Use filters to narrow scope** - don't outline an entire large repo without `--kind`, `--lang`, or `--pattern`
4. **Don't over-constrain budgets** - use `--kind`, `--lang`, `--pattern`, and `--public` as primary scope controls; budget is a safety net
5. **Use outline results to target reads** - the line numbers in outline output tell you exactly where to Read
6. **Prefer `--pattern` over reading entire layers** when you know the area of interest
7. **Pipe to json when parsing programmatically** - use `xray outline -f json | ...` if you need structured data

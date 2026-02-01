# literate-lsp

**Your code blocks deserve IDE superpowers.**

Ever copy code out of your documentation just to get autocomplete and type hints? Stop. Literate-lsp brings full language intelligence directly to embedded code blocks - hover hints, goto definition, completions - all inline where you're writing.

Write documentation with confidence. Code with precision.

## What It Does

You write code in markdown/typst blocks:

````markdown
# How to Define a Square in Forth

```forth
: square ( n -- n ) dup * ;
```

Now use it:

```forth
5 square .
```
````

Literate-lsp gives you IDE features _inside those blocks_:

- 🎯 **Goto definition** - Jump from `square` to its definition
- 💡 **Hover hints** - See types, signatures, documentation
- ✨ **Completions** - Autocomplete and snippets
- 🔍 **Find references** - See where symbols are used

All while keeping your documentation as the source of truth.

## Quick Start

1. Install: `cargo install literate-lsp`
2. Configure your editor to use it as an LSP
3. Open your markdown/typst files
4. Start getting IDE help in your code blocks

### Helix Integration

Add to your `~/.config/helix/languages.toml`:

```toml
[language-server.literate-lsp]
command = "literate-lsp"

[[language]]
name = "markdown"
language-servers = ["literate-lsp"]
```

Or create a local `languages.toml` in your project to override LSP settings per-repository.

## Supported Languages

Literate-lsp works with **any language** that has an LSP. Out of the box:

- **Forth** via forth-lsp
- **Go** via gopls
- **Rust** via rust-analyzer
- **TypeScript/JavaScript** via typescript-language-server
- **Python** via pylsp or pyright
- **And 100+ more** from your Helix config

## Check Your Setup

```bash
# See which LSPs are installed and ready
literate-lsp --health

# List all configured LSPs
literate-lsp --languages

# Check a specific language
literate-lsp --health rust

# Generate detailed logs
RUST_LOG=literate_lsp=debug hx README.md
```

## Configuration

Literate-lsp reads from Helix's `languages.toml` - the same config file you already use. It automatically picks up:

- LSP binaries and paths
- Language-specific settings (hints, formatting options, etc.)
- Custom initialization parameters

Create a local `./.languages.toml` in your project to override settings for that repo only.

## The Why

Literate programming isn't just about documentation - it's about making code intelligible. But we've been forcing a false choice: either treat your code blocks as mere text, or copy them out to a "real" file to get IDE support.

Literate-lsp closes that gap. Your documentation becomes your development environment.

## How It Works

1. **Extracts** code blocks from your markdown/typst by language
2. **Builds** a virtual document containing just the code
3. **Delegates** to the appropriate language server (gopls, forth-lsp, etc.)
4. **Maps** results back to your original document coordinates
5. **Shows** IDE features right where you wrote the code

Everything happens transparently. Your files stay as-is.

## Status

✅ Production-ready for Helix
✅ Works with any LSP
✅ Fast parallel health checking
✅ Zero configuration needed (uses Helix config)

## Learn More

- **Forth Example**: Check `./example.md` for a mini demo using Forth
- **Virtual documents**: Check `/tmp/virtual.*` files while editing to see what's being sent to LSPs
- **Architecture**: Read `CLAUDE.md` for implementation details

---

**Your code blocks are too smart to stay dumb.**

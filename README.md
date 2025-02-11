# TechDocs

A Rust-based CLI tool for generating technical documentation from codebases, with support for AI-powered README generation.

## Key Features

- Recursive directory traversal with `.gitignore` support
- Smart file filtering and size limits
- UTF-8 file content validation
- AI-powered README generation using Claude API
- Customizable exclude patterns
- Language-aware code formatting

## Installation

```bash
# Clone the repository
git clone https://github.com/thesurlydev/techdocs.git

# Build the project
cargo build --release

# Add to path (optional)
cp target/release/techdocs /usr/local/bin/
```

Requires:
- Rust toolchain
- `ANTHROPIC_API_KEY` environment variable for Claude integration

## Usage

```bash
# Generate formatted content for AI prompts
techdocs prompt -p /path/to/project --max-size 100 --total-size 10

# Generate README using Claude AI
techdocs readme -p /path/to/project
```

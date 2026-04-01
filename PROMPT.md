# linux-vorpal Cross-SDK Parity — Resume Prompt

## Context

We're implementing the `linux-vorpal` artifact in Go and TypeScript SDKs to match the existing Rust SDK with byte-identical digests. The work is ~90% complete. The codegen tool, LinuxDebian ports, Shell() integrations, and TS escape fixes are all done. What remains is fixing the Go codegen output and final verification.

## What's Been Done

### Architecture
- **Codegen tool** at `tools/linux-vorpal-codegen/` reads Rust script files and generates Go + TS equivalents
- **LinuxDebian** ported to both Go (`sdk/go/pkg/artifact/linux_debian.go`) and TS (`sdk/typescript/src/artifact/linux_debian.ts`)
- **Shell() integration** updated in both Go (`sdk/go/pkg/artifact/step.go`) and TS (`sdk/typescript/src/artifact/step.ts`) to call local builders instead of `FetchArtifactAlias`
- Go uses a **registration pattern** (`linux_vorpal/register.go` + blank import in `cmd/vorpal/main.go`) to avoid import cycles
- `make generate` now runs the codegen tool after protobuf generation

### Bugs Found and Fixed
1. **Go `json.Marshal` HTML-escapes** `&`, `<`, `>` — fixed with `SetEscapeHTML(false)` in `sdk/go/pkg/config/artifact_serializer.go`
2. **Rust `\<newline>` string continuations** collapse lines in Dockerfile strings — Go and TS `generateDockerfile()` now use collapsed single-line RUN commands
3. **TS `linux_debian.ts` EOF newline** — fixed `${versionScript}EOF` to `${versionScript}\nEOF`
4. **TS template literal backslash escapes** — `\*`, `\.`, `\n` (in sed) silently drop backslash in JS. Fixed in codegen tool (`main.rs` line ~513) by escaping `\` as `\\` in TS output
5. **Codegen tool Go import paths** — fixed wrong casing and paths in `main.rs`

### Current Digest Status
- **linux-debian-dockerfile**: All three SDKs match (`e8fe71f4...`) ✅
- **linux-debian**: All three SDKs match (`da8c6011...`) ✅  
- **linux-vorpal (TS)**: Last TS digest was `cc355c0f...`, Rust is `cfaa20f7...` — the TS backslash fix in the codegen should resolve this but hasn't been re-tested since the codegen fix
- **linux-vorpal (Go)**: Not tested yet — Go codegen output has compilation errors

## What Remains

### 1. Fix Go Codegen Output (CRITICAL)

The Go orchestration generator in `tools/linux-vorpal-codegen/src/main.rs` produces code with these errors (from `generate_go_orchestration` and related functions):

```
linux_vorpal.go:90:42: cannot convert ctx to type artifact.LinuxDebian
linux_vorpal.go:198:72: undefined: api.ArtifactSystem_X86_64_LINUX
linux_vorpal.go:200:11: invalid operation: cannot call make (shadows builtin)
linux_vorpal.go:337:9: cannot use *string as string in return
register.go:8:32: function signature mismatch (string vs *string)
```

**Fixes needed in the codegen:**
- `ArtifactSystem_X86_64_LINUX` → `ArtifactSystem_X8664_LINUX` (matching protobuf)
- `artifact.LinuxDebian(ctx)` → `artifact.NewLinuxDebian().Build(ctx)` (struct method pattern)
- Variable `make` → `makeSrc` (avoid shadowing Go builtin)
- Return type should be `(*string, error)` not `(string, error)`
- Import paths: use `api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"` with alias

These were manually fixed by DKT-117 in the worktree but need to be baked into the codegen generator so `make generate` produces correct Go code.

### 2. Remove Debug Prints

Remove temporary debug prints added for digest comparison:
- `sdk/rust/src/context.rs` (~line 328): `eprintln!` for linux-debian/linux-vorpal
- `sdk/typescript/src/context.ts` (~line 668): `console.error` for linux-debian/linux-vorpal  
- `sdk/go/pkg/config/context.go` (~line 393): `fmt.Printf` for linux-debian/linux-vorpal

### 3. Verify TS Digest Parity

After the codegen backslash fix, regenerate and test:
```bash
cargo run -p linux-vorpal-codegen
# Then build in Lima VM:
TERM="xterm-256color" limactl shell vorpal-aarch64 bash -c "cd ~/vorpal && target/debug/vorpal build --config 'Vorpal.ts.toml' vorpal"
```

The TS linux-vorpal digest should now match Rust's `cfaa20f7e8d2d12a3d81798edb4d93611c455edaeea439ba87bf2ac9f821ab75`.

### 4. Verify Go Digest Parity

After fixing Go codegen, regenerate and test:
```bash
cargo run -p linux-vorpal-codegen
# Then build in Lima VM:
TERM="xterm-256color" limactl shell vorpal-aarch64 bash -c "cd ~/vorpal && target/debug/vorpal build --config 'Vorpal.go.toml' vorpal"
```

## Key Files

| File | Purpose |
|------|---------|
| `tools/linux-vorpal-codegen/src/main.rs` | Codegen tool — reads Rust scripts, generates Go/TS |
| `sdk/go/pkg/artifact/linux_debian.go` | Go LinuxDebian builder (hand-written) |
| `sdk/go/pkg/artifact/linux_vorpal/` | Go linux-vorpal files (generated + register.go) |
| `sdk/go/pkg/artifact/step.go` | Go Shell() integration |
| `sdk/go/cmd/vorpal/main.go` | Go blank import for registration |
| `sdk/go/pkg/config/artifact_serializer.go` | Go JSON serializer (SetEscapeHTML fix) |
| `sdk/typescript/src/artifact/linux_debian.ts` | TS LinuxDebian builder (hand-written) |
| `sdk/typescript/src/artifact/linux_vorpal/` | TS linux-vorpal files (generated) |
| `sdk/typescript/src/artifact/step.ts` | TS shell() integration |
| `sdk/typescript/src/context.ts` | TS serializer and addArtifact |
| `sdk/rust/src/artifact/linux_vorpal/` | Rust source of truth (scripts, sources, orchestration) |
| `sdk/rust/src/context.rs` | Rust addArtifact with debug print |
| `Makefile` | `make generate` runs protobuf + codegen |

## Important Technical Notes

1. **Rust `formatdoc!` `\\` = literal `\`** — the Rust lexer processes `\\` before formatdoc sees it
2. **Rust `\<newline><whitespace>` = line continuation** — collapses in string literals, not visible to formatdoc
3. **Go raw strings (backticks) preserve `\` literally** — no escape processing, so Go is fine
4. **TS template literals silently drop `\` before unrecognized escapes** — `\*`→`*`, `\.`→`.`, but `\n`→newline (wrong for sed's `\n`)
5. **Go `json.Marshal` HTML-escapes `&<>`** — must use `json.NewEncoder` with `SetEscapeHTML(false)`
6. **Builds run in Lima VM** (`limactl shell vorpal-aarch64`) not locally
7. **Use `bun x` not `npx`** for TypeScript tooling

## TDD Reference

Full design document: `docs/tdd/linux-vorpal-cross-sdk-parity.md`

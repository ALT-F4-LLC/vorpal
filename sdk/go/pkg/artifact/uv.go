package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

const defaultUvVersion = "0.10.11"

// Uv builds the Astral uv toolchain (standalone release).
//
// HASH-ENFORCEMENT (C3 foundation): on "uv sync --frozen", uv-0.10.11 verifies each
// package against the per-package hashes in uv.lock and rejects any content-hash
// mismatch. There is no "uv sync --require-hashes" CLI flag (--require-hashes is uv's
// pip-interface flag); the hashed-lock verification IS the enforcement surface.
//
// PROVENANCE — no inline WithDigest (ADR 0001 Part A). The canonical pin is the per-triple
// Vorpal.lock entry captured via --unlock; until then the HTTP source is intentionally
// unpinned and the C1 mint gate fails the build closed. See Cpython for the full
// two-link provenance rationale.
func Uv(context *config.ConfigContext) (*string, error) {
	name := "uv"

	system := context.GetTarget()

	sourceTarget, err := CpythonTarget(system)
	if err != nil {
		return nil, err
	}

	sourceVersion := defaultUvVersion
	sourcePath := fmt.Sprintf(
		"https://sdk.vorpal.build/source/uv-%s-%s.tar.gz",
		sourceVersion, sourceTarget,
	)

	source := NewArtifactSource(name, sourcePath).Build()

	// Astral standalone release unpacks to uv-{triple}/uv at the tarball root.
	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/%s/uv-%s/uv" "$VORPAL_OUTPUT/bin/uv"
chmod +x "$VORPAL_OUTPUT/bin/uv"
`, name, sourceTarget)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, systems).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}

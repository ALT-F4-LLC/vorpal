package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

const defaultPythonVersion = "3.13.14"

// CpythonTarget maps a Vorpal ArtifactSystem to the python-build-standalone target triple.
// Exported for reuse by uv and other python-toolchain artifacts.
func CpythonTarget(system api.ArtifactSystem) (string, error) {
	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN:
		return "aarch64-apple-darwin", nil
	case api.ArtifactSystem_AARCH64_LINUX:
		return "aarch64-unknown-linux-gnu", nil
	case api.ArtifactSystem_X8664_DARWIN:
		return "x86_64-apple-darwin", nil
	case api.ArtifactSystem_X8664_LINUX:
		return "x86_64-unknown-linux-gnu", nil
	default:
		return "", fmt.Errorf("unsupported target system: %s", system.String())
	}
}

// Cpython builds the python-build-standalone relocatable interpreter (install_only).
//
// Source name is "cpython", NOT "python" — the existing linux_vorpal bootstrap owns a
// "python" source compiled from source; sources key by (name, platform), so reusing
// "python" would collide (ADR 0001 / TDD §4, H1).
//
// PROVENANCE — no inline WithDigest (ADR 0001 Part A). The canonical pin is the per-triple
// Vorpal.lock entry captured via --unlock; until then the HTTP source is intentionally
// unpinned and the C1 mint gate fails the build closed. A placeholder digest is
// intentionally avoided: agent.rs returns an inline digest on a registry-cache hit without
// verifying content, so a predictable placeholder is a pre-seedable cache-poison key.
func Cpython(context *config.ConfigContext) (*string, error) {
	name := "cpython"

	system := context.GetTarget()

	sourceTarget, err := CpythonTarget(system)
	if err != nil {
		return nil, err
	}

	sourceVersion := defaultPythonVersion
	sourcePath := fmt.Sprintf(
		"https://sdk.vorpal.build/source/cpython-%s-%s.tar.gz",
		sourceVersion, sourceTarget,
	)

	source := NewArtifactSource(name, sourcePath).Build()

	// pbs install_only tarballs unpack to a top-level python/ dir at the tarball root.
	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT"
cp -prf "./source/%s/python/." "$VORPAL_OUTPUT/"
`, name)

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

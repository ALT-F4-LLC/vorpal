package artifact

import (
	"errors"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func RustToolchainTarget(system *api.ArtifactSystem) (*string, error) {
	var target string

	switch *system {
	case api.ArtifactSystem_AARCH64_DARWIN:
		target = "aarch64-apple-darwin"
	case api.ArtifactSystem_AARCH64_LINUX:
		target = "aarch64-unknown-linux-gnu"
	case api.ArtifactSystem_X8664_DARWIN:
		target = "x86_64-apple-darwin"
	case api.ArtifactSystem_X8664_LINUX:
		target = "x86_64-unknown-linux-gnu"
	default:
		return nil, errors.New("unsupported 'rust-toolchain' system")
	}

	return &target, nil
}

func RustToolchainVersion() string {
	return "1.83.0"
}

func RustToolchain(context *config.ConfigContext) (*string, error) {
	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	var digest string

	switch *target {
	case api.ArtifactSystem_AARCH64_DARWIN:
		digest = "84707c7325d3a0cbd8044020a5256b6fd43a79bd837948bb4a7e90d671c919e6"
	case api.ArtifactSystem_AARCH64_LINUX:
		digest = "ad490acd52f5b4d5b539df8f565df3a90271225a1ef6256c1027eac0b70cb4d4"
	case api.ArtifactSystem_X8664_DARWIN:
		digest = "589c625bd79be3ed8b9d5168c54a889dba971a6e9d9722750c4b4577247ec94e"
	case api.ArtifactSystem_X8664_LINUX:
		digest = "5442c5e085972b7119661da12d03d40fb17770edf8879ab898aee3dafdd1c48c"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

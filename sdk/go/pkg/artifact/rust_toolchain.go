package artifact

import (
	"errors"
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func RustToolchainTarget(system api.ArtifactSystem) (*string, error) {
	var target string

	switch system {
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
	return "1.89.0"
}

func RustToolchain(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias(fmt.Sprintf("rust-toolchain:%s", RustToolchainVersion()))
}

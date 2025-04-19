package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Goimports(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "66a42cc7600ef08f1937ff314c36cceec26451630e83b6c2d6a8f93bf7291b59"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "1d48d6a3d0ff9ffa616e6b152c8aa4ca34f4db49e5a9adfdbb0c987235d3aade"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "e5d1d90c5d5bc629a25da5e856c6bf5ddc754a46745718c2a60fe8c404819c52"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "1ad022e6c026105286866402ca348ccddfd2a56926361dd7b82cc8c26de183c1"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

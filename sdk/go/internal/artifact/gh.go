package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Gh(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "a038ac34aeef1ac2acabafe4a99312e88bc4a611746d23d14c179b81123aa25e"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "57bb095cfcbfabbb06129e01463fd7162e7f60b09f0b39d86295c9eca35a75c6"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

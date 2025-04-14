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
		digest = "112f8c42be33bfa5274fcdff2748cd68eae755adbae1cbd70cc012531375d7c1"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "6562e6c4c50f955c7d808860b383261446525f1bcfccd1b9ac5f8f58af7d8842"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func ProtocGenGoGRPC(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "d4165e4b2ca1b82ddaed218d948ce10eca9a714405575ffccb0674eb069e3361"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "ee99a33b1d55752ff6dc8449af4b8ac1df89e0aaf72f932e41690221bba5459c"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

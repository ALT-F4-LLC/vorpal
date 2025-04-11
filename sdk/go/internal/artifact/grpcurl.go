package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Grpcurl(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "718ab77ba1f560c9d585e4914fac47d63494c54707a6d25c93a3ee0a9434b092"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = ""
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = ""
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

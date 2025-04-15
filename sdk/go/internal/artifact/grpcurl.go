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
		digest = "29aad45ba08242e0b3120a34560abc5cc14c5b73e622659b119ab266bb4ea5b8"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "021260395071224bc434fe02afabaedd3cb5ea9fc727c6d17990a4fda5de88ba"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "d435a3ffead3cf624243cd4d93e5160b04e7e05188e87ed8cb0972230549a116"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

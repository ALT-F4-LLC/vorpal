package artifact

import (
	"errors"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Grpcurl(context *config.ConfigContext) (*string, error) {
	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	var digest string

	switch *target {
	case api.ArtifactSystem_AARCH64_DARWIN:
		digest = "292c9b6a2d40fcddf8add7533c96951e0d60d756b4a57c72093ab4be74bb0ce7"
	case api.ArtifactSystem_AARCH64_LINUX:
		digest = "1a9c61d9af34ae6d95eb85e63fa49dff18393269b0f266f3a3adf0f68238e2d8"
	case api.ArtifactSystem_X8664_DARWIN:
		digest = "b8dacfe7be0747a87bc1278bbb3ff2179702314de3cac74f75ee61047786350b"
	case api.ArtifactSystem_X8664_LINUX:
		digest = "22c3ed96cbd3e6fcec8b424e91ecd9f8b332a92e372c159f2edc0ec8f34f6c27"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

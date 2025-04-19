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
		digest = "292c9b6a2d40fcddf8add7533c96951e0d60d756b4a57c72093ab4be74bb0ce7"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "109a4ed11d63bf7f9afbd819342ef8e2988873ee146e525d65e7416928855ddf"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "22c3ed96cbd3e6fcec8b424e91ecd9f8b332a92e372c159f2edc0ec8f34f6c27"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

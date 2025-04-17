package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Gopls(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "da002e70fe77900217324968e5a738673bd9a3b005d53f9455c8852ac5a2a315"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "00f678057bbc3e7fa6e2df780c8064e751cae19c8406c1c1ab96de8a1a43b66d"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "fc473a2d04a3ccff623d23bd9bcfd38347e7bfc5de0a5b390941a1808f1758ce"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

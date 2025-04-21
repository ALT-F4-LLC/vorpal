package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Nginx(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "5a78e58e9dbc4915194b7ce68bcddf4b70e212a81813a66f5f3e47d84332ccfa"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "26a9f79b00db4e79b25511b739bccb2ca34f6374c539a33954c234b53f282d52"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "12d14d4d5675e5c17ac1d47fd6b69813bab2814b93c17a10b83857cafdff1671"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "19f8c5223fcbc5db0316dedc205505e30e1475e3c35e9178533fba00b5e5006d"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

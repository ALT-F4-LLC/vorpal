package artifact

import (
	"errors"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Staticcheck(context *config.ConfigContext) (*string, error) {
	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	var digest string

	switch *target {
	case api.ArtifactSystem_AARCH64_DARWIN:
		digest = "d06c462ddccb64f6838276c9bcd987f65411c95d2fd9b7944d070e6014f7c40b"
	case api.ArtifactSystem_AARCH64_LINUX:
		digest = "3d8dd1e8d2040415cfda963b56d6acaf888415c697b4c315ca186a88858eab87"
	case api.ArtifactSystem_X8664_DARWIN:
		digest = "d81cc5e018b5b65f6f26a17440a317fefd6bfc26a3cd62316bf92d57aafa837f"
	case api.ArtifactSystem_X8664_LINUX:
		digest = "d88da046d4fdc9833f4577263f0144cdda8e4014627232e2bcf9f9a85889fa0d"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

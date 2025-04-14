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
		digest = "91cc5dc188bc8bc5e6ff63aede51540ae15c7980ab898e1b0afb6db79657fa43"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "be4a64072d90e74eb5eb071c03003d42201e33298cb2152352054413a96478aa"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

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
		digest = "aca4873c1116603151fd3179855f1000030e10e0428f6e4f8f326bde7f3e74f9"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "9072f0f63cb2f4aa6f0b16a6122a986b0852f7acfe47e40e4a6ded3bdf1cb909"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "53a903ec95b4c5c803261d3b74d11bc795990fbf7835fe6a067c8207335cc1f1"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

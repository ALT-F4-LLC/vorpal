package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Staticcheck(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "aff57d058fa0e464db749fa2da8666cc84d60c66ce21607e9ab88c204e5bc50e"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "5276342a07693cefad1c81781c911e23663792cfa3ee93dc52840c5807c499ca"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "593863f93db731378260ad19568f94f8ee7dc0b8a4a2068fe5a29aee82df5437"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "fd95a2ed396051b3479afb9ecea6e3722aea6047f956fb87828c1e5aed64391f"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func ProtocGenGo(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "47a94d59d206be31eef2214418fce60570e7a9a175f96eeab02d1c9c3c7d0ed9"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "2f7455a7197f272a0647f8ba466eb3abb56898eb8979eea8af49479cce3e1153"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = ""
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

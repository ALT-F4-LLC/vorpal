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
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "0a493af2551398c22cc36c3aad51bf0dcee8b9b8d78a58a04a521f15a63f6b46"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

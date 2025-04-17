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
		digest = "eb74536ca2985d1a22d2458b12750c5ef6ff5824fd5d4162133c8a1b025e3679"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "3c9fcea77069bdfc93bbfbaffc6cb66c62ab9b5325a263b05709bc8ec9758116"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "cc1b2b31f4051cb9d9d856c3ed3fbe622ee5de74659543d67184c4a9d5c8c557"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "5d241c0eab2de3e0ad35f04006289957fd5ee4d846f2ae6c8de12c7101344bff"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

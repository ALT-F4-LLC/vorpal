package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Protoc(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "8ad451bdcda8f24f4af59ccca23fd71a06975a9d069571f19b9a0d503f8a65c8"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "d0f3c08c81bebdb5a502863c786a03d661e4faad1941e941f705bb076eaff13c"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "2a3d7816b06f1a046cbf8e82c1a94fe71b4fd384726f2064c9e0960ac75dadec"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "<TODO>"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

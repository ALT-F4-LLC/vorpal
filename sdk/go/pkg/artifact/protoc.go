package artifact

import (
	"errors"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Protoc(context *config.ConfigContext) (*string, error) {
	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	var digest string

	switch *target {
	case api.ArtifactSystem_AARCH64_DARWIN:
		digest = "8ad451bdcda8f24f4af59ccca23fd71a06975a9d069571f19b9a0d503f8a65c8"
	case api.ArtifactSystem_AARCH64_LINUX:
		digest = "56abea3fb5be73d12c5bc75bae48451c6ee625b4d727c905f92d454286d4ea65"
	case api.ArtifactSystem_X8664_DARWIN:
		digest = "2a3d7816b06f1a046cbf8e82c1a94fe71b4fd384726f2064c9e0960ac75dadec"
	case api.ArtifactSystem_X8664_LINUX:
		digest = "c5bded4de6ca52ac5e731e328e22b5dfb957009ed5a553e24ba9fdec4379ba44"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

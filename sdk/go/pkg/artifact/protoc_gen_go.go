package artifact

import (
	"errors"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func ProtocGenGo(context *config.ConfigContext) (*string, error) {
	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	var digest string

	switch *target {
	case api.ArtifactSystem_AARCH64_DARWIN:
		digest = "47a94d59d206be31eef2214418fce60570e7a9a175f96eeab02d1c9c3c7d0ed9"
	case api.ArtifactSystem_AARCH64_LINUX:
		digest = "898d6da16c8799e8f1789013d0aa36ae3c76293fe413650d7e63b988d0fc879e"
	case api.ArtifactSystem_X8664_DARWIN:
		digest = "c111d8788b4e1109be52b2f4c2ba9ed8c42831ea0e8ed67730fefe61d1b4bd6b"
	case api.ArtifactSystem_X8664_LINUX:
		digest = "5bdb70e7412dc35c61a706b8a623978f0f8ff1ff11e9b5a31ef4b49dfb71a6df"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}

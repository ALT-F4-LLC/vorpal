package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func GoBin(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "6f024d78f0957297229cb00b74b9544fb2c4708a465a584b1e02dfbe5f71922b"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "42c82308fb915d08bdec4c9bb9d89f4e96fcaaab5e42af9e7e8137880001d1c6"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "ea09f27786bb3eb7d91425c08ac7098b2192c32cbb5c1a2196f5c8e4a9a4d0ff"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "<TODO>"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
